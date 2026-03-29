# Context Engineering — Codex Deep Research

## Why This Matters

Context engineering is the #1 silent killer of agent quality. The model can only reason
about what's in its context window. Too much noise → the model loses focus. Too little
context → the model hallucinates. Wrong prioritization → the model fixates on stale
information while missing critical recent context.

Most agents just stuff everything in and hope for the best. Codex implements a
**three-phase assembly pipeline** with dynamic compaction, differential updates, and
budget-aware truncation.

---

## Three-Phase Input Assembly

### Phase 1: Full Initial Context Injection

**File**: `core/src/codex.rs:3167-3287` (`build_initial_context()`)

On the first turn (or after compaction), the system builds a complete context from scratch:

```
System prompt assembly:
  1. Base instructions (model-specific, ~5K words)
  2. Permissions & approval policy description
  3. Developer instructions (AGENTS.md, skills, personality)
  4. Memory tool availability
  5. Collaboration mode template
  6. Realtime status (if applicable)
  7. Environment context XML
  8. Commit message conventions (if applicable)
```

This is expensive — the full system prompt alone can consume 5-10K tokens. But it only
happens once per session (or after compaction).

### Phase 2: Differential Settings Updates

**File**: `core/src/context_manager/updates.rs:177-213` (`build_settings_update_items()`)

On subsequent turns, the system computes a **diff** against the previous context:

```
reference_context = last_injected_context
current_context = build_from_current_session_state()

diff = current_context - reference_context
if diff is empty:
    emit nothing  // No changes, save tokens
else:
    emit only changed sections
```

**What triggers a diff update**:
- Model switch
- Permission/approval policy change
- Collaboration mode change
- Realtime status change
- Personality change

**What's NOT re-emitted** (unless changed): Base instructions, environment context,
memory tool config — all stable within a session.

### Phase 3: Conversation History

**File**: `core/src/context_manager/history.rs`

The `ContextManager` maintains an ordered list of items (oldest→newest):

```
[initial_context] [user_msg_1] [assistant_msg_1] [tool_call_1] [tool_output_1]
[user_msg_2] [assistant_msg_2] [tool_call_2] [tool_output_2] ...
```

Items are appended after each model response and tool execution. The full history
becomes the `input` array in the Responses API call.

---

## Token Estimation

**File**: `core/src/truncate.rs`

### The 4-Byte Heuristic

```rust
const APPROX_BYTES_PER_TOKEN: usize = 4;

fn estimate_tokens(text: &str) -> usize {
    text.len() / APPROX_BYTES_PER_TOKEN
}
```

Codex deliberately uses a **fast approximation** instead of real tokenization:
- Deterministic (no tokenizer version dependency)
- Fast (no BPE computation)
- Good enough for budget decisions (within ~15% of actual)

### Item-Level Estimation

For each conversation item:
1. Serialize to JSON
2. Discount image payloads (7,373 bytes ≈ 1,844 tokens per image)
3. Apply reasoning decompression ratio (3:4 from base64)
4. Divide by 4

### Cumulative Tracking (`TotalTokenUsageBreakdown`)

```rust
pub struct TotalTokenUsageBreakdown {
    last_api_response_total_tokens: i64,
    all_history_items_model_visible_bytes: usize,
    estimated_tokens_of_items_added_since_last_api_response: i64,
    estimated_bytes_of_items_added_since_last_api_response: usize,
}
```

The system tracks both the last *actual* token count from the API and its own *estimated*
count of items added since. This gives a running estimate without re-counting everything.

---

## Auto-Compaction

### Triggers

**File**: `core/src/codex.rs`

Auto-compaction fires when:
1. `total_tokens >= model.auto_compact_token_limit` (pre-turn check)
2. Model switches to a smaller context window
3. `ContextWindowExceeded` error during turn execution

### Pre-Turn Compaction

**File**: `core/src/compact.rs:54-232`

The model generates a summary of the conversation so far:

```
SUMMARIZATION_PROMPT:
  "Summarize the conversation so far. Preserve:
   - All file paths, function names, variable names mentioned
   - Key decisions and their rationale
   - Current state of the task
   - Any errors encountered and their resolutions"
```

### Post-Compaction Reconstruction

After the model returns a summary:

1. **Initial context** → reinjected fresh from current session state (not from history)
2. **Recent user messages** → placed before the summary (up to 20K token budget)
3. **Summary** → placed last
4. **Ghost snapshots** → re-added (commit state markers)
5. **Older user messages** → selected within budget

**Dropped**: All tool calls/outputs/reasoning before the summary point, stale developer
messages, old assistant messages.

### Mid-Turn Compaction

If `ContextWindowExceeded` occurs *during* a turn:

1. Progressive oldest-item removal (not model-based summarization)
2. Removes items from the beginning of history
3. `InitialContextInjection::BeforeLastUserMessage` — reinjects context before
   the most recent user message

This is faster than full compaction but less intelligent — it's an emergency measure.

---

## Tool Output Truncation

**File**: `core/src/truncate.rs`

### Truncation Policy

Each model defines a truncation policy:

```rust
pub struct TruncationPolicy {
    max_tool_output_tokens: usize,
    // 20% buffer added for JSON serialization overhead
}
```

### Prefix + Suffix Preservation

When output exceeds the limit, Codex keeps both the **beginning** and **end**:

```
[first N chars] ... <truncated {removed_count} chars> ... [last M chars]
```

This is deliberate: error messages often appear at the end of output, while command
identification appears at the beginning. Keeping only the prefix would lose errors.

### Multi-Item Output Handling

For tool outputs with multiple items (e.g., directory listings):
1. Keep as many **full** items as possible
2. Truncate only the **last** item that doesn't fit
3. Never produce partial items in the middle

---

## Context Diffs: The Efficiency Multiplier

The differential update mechanism is one of Codex's most impactful optimizations.

### How It Works

A `reference_context_item` tracks the baseline:

```
Turn 1: Full inject (reference_context = turn_1_context)
Turn 2: Diff = current - reference → nothing changed → emit 0 tokens
Turn 3: User changes approval policy → emit only permission section (~200 tokens)
Turn 4: Diff = current - reference → nothing changed → emit 0 tokens
```

Without diffs, every turn would re-emit the full system prompt (5-10K tokens).
With diffs, most turns emit 0 additional context tokens.

### When Full Reinject Happens

- First turn of a session
- After compaction (reference_context is cleared)
- After model switch (instructions may differ)

---

## Priority During Compaction

Not all context is equal. The retention priority is:

1. **User messages** — highest priority (authorization signal, task definition)
2. **Recent assistant messages** — context for current work
3. **Tool outputs** — evidence of what was done
4. **Older conversation** — summarized and dropped
5. **Stale developer instructions** — replaced with fresh inject

### User Message Budget

During compaction, user messages get a dedicated **20K token budget**:

```
for msg in user_messages.reverse():
    if budget_remaining > msg.tokens:
        retain(msg)
        budget_remaining -= msg.tokens
    else:
        break
```

This ensures the model always has access to the user's original request and recent
instructions, even after aggressive compaction.

---

## Design Insights

1. **Estimate, don't count.** Real tokenization is expensive and version-dependent.
   The 4-byte heuristic is fast, deterministic, and accurate enough for budget decisions.
   Only the API's actual token count (returned in responses) is used for precise tracking.

2. **Diff-mode saves more tokens than you'd think.** In a typical 20-turn session, the
   system prompt is emitted once. Without diffs, it would be emitted 20 times — a 20x
   overhead on the most expensive part of the context.

3. **Prefix + suffix truncation is superior to prefix-only.** Error messages, stack traces,
   and exit codes appear at the end. Build logs and command identification appear at the
   beginning. Both are valuable.

4. **Compaction is a two-tier system.** Pre-turn compaction uses model-based summarization
   (smart but slow). Mid-turn compaction uses progressive removal (fast but dumb). This
   matches the urgency: pre-turn has time for intelligence, mid-turn needs immediate action.

5. **Fresh reinject beats stale history.** After compaction, initial context is rebuilt
   from *current session state*, not from the old history. This prevents stale permission
   descriptions or outdated environment context from persisting.

6. **User messages are sacred.** The 20K budget for user messages during compaction ensures
   the model never loses sight of what the user actually asked for. This is the #1 cause
   of "agent drift" in other systems.

---

## Key Files

| Component | Path |
|-----------|------|
| Full context injection | `core/src/codex.rs:3167-3287` |
| Differential updates | `core/src/context_manager/updates.rs:177-213` |
| History management | `core/src/context_manager/history.rs` |
| Token estimation | `core/src/truncate.rs` |
| Auto-compaction logic | `core/src/compact.rs:54-232` |
| API assembly | `core/src/client.rs:490-556` |
| Token usage tracking | `core/src/context_manager/history.rs` (TotalTokenUsageBreakdown) |
