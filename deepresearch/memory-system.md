# Memory System: Cross-Session Agent Learning

Deep-dive into how Codex implements long-term memory — a two-phase asynchronous
pipeline that turns individual session rollouts into consolidated, file-based
agent knowledge that persists and improves across all future sessions.

---

## 1. Mental Model

```
 Session 1   Session 2   Session 3   Session 4
    │            │            │            │
    ▼            ▼            ▼            ▼
 rollout.jsonl  rollout.jsonl rollout.jsonl rollout.jsonl
    │            │            │            │
    └────────────┴────────────┴────────────┘
                       │
               ┌───────┴────────┐
               │   PHASE 1      │  gpt-5.1-codex-mini (fast, per-thread)
               │   Extraction   │  "What's worth remembering?"
               └───────┬────────┘
                       │
                  stage1_outputs (SQLite)
                       │
               ┌───────┴────────┐
               │   PHASE 2      │  gpt-5.3-codex (powerful, global singleton)
               │  Consolidation │  "Organize everything into a handbook"
               └───────┬────────┘
                       │
              ~/.codex/memories/
              ├── memory_summary.md        ← Always loaded (5K tokens)
              ├── MEMORY.md                ← Searched on demand
              ├── rollout_summaries/*.md   ← Per-rollout detail
              └── skills/*/SKILL.md        ← Reusable procedures
                       │
               ┌───────┴────────┐
               │   RETRIEVAL    │  Any agent turn
               │   read_path   │  "Here's what memory is available"
               └────────────────┘
```

**Key design**: Memories are **never injected directly** into the model prompt.
Instead, the model gets a `memory_summary.md` snippet + instructions for how to
look up more detail via shell/read tools. This is **progressive disclosure**.

---

## 2. Activation & Triggers

### Entry Point: `start_memories_startup_task`

```rust
// codex-rs/core/src/memories/start.rs
pub(crate) fn start_memories_startup_task(
    session: &Arc<Session>,
    config: Arc<Config>,
    source: &SessionSource,
)
```

**Activation criteria** (ALL must be true):
- NOT ephemeral session (`config.ephemeral == false`)
- Memory feature enabled (`Feature::MemoryTool`)
- NOT a sub-agent session
- State DB available

**Flow** (async, non-blocking):
1. `phase1::prune()` — remove stale unused memories
2. `phase1::run()` — extract new memories from recent rollouts
3. `phase2::run()` — consolidate into filesystem artifacts

---

## 3. Configuration

```rust
// codex-rs/core/src/config/types.rs
pub struct MemoriesConfig {
    pub generate_memories: bool,                   // Phase 1 enabled
    pub use_memories: bool,                        // Inject into prompts
    pub no_memories_if_mcp_or_web_search: bool,   // Skip if external tools
    pub max_raw_memories_for_consolidation: usize, // Phase 2 input cap (default: 256)
    pub max_unused_days: i64,                      // Retention window (default: 30)
    pub max_rollout_age_days: i64,                 // Only recent rollouts (default: 30)
    pub max_rollouts_per_startup: usize,           // Per-startup claim cap (default: 16)
    pub min_rollout_idle_hours: i64,               // Cooldown before extraction (default: 6)
    pub extract_model: Option<String>,             // Override phase-1 model
    pub consolidation_model: Option<String>,       // Override phase-2 model
}
```

```toml
# Example config.toml
[memories]
generate_memories = true
use_memories = true
max_raw_memories_for_consolidation = 256
max_unused_days = 30
max_rollouts_per_startup = 16
min_rollout_idle_hours = 6
```

---

## 4. Phase 1: Rollout Extraction (Per-Thread)

### Constants

| Parameter | Value |
|---|---|
| Model | `gpt-5.1-codex-mini` |
| Reasoning effort | Low |
| Concurrency | 8 parallel jobs |
| Rollout token limit | 150,000 tokens |
| Context window usage | 70% |
| Job lease | 3,600 seconds |
| Retry delay | 3,600 seconds |
| Thread scan limit | 5,000 |
| Prune batch size | 200 |

### Job Claim Logic

```
for thread in query_stale_threads(max_age=30d, min_idle=6h, scan_limit=5000):
    if claimed.len() >= 16: break
    if thread.source NOT IN [cli, ui, webui]: skip
    if thread has fresh stage1_output: skip (SkippedUpToDate)
    if thread job is running: skip (SkippedRunning)
    if thread job exhausted retries: skip (SkippedRetryExhausted)
    claim_with_exclusive_ownership(thread)
    claimed.push(thread)
```

### Extraction Pipeline

```
1. Load rollout JSONL from thread.rollout_path
        │
2. Filter: should_persist_response_item_for_memories(item)
   (keep messages, tool calls; redact secrets)
        │
3. Truncate content to fit model context:
   limit = context_window × 70% × effective_context_window_percent
   (preserves head + tail when truncating)
        │
4. Build prompt:
   System: stage_one_system.md (~337 lines of extraction instructions)
   User:   stage_one_input.md (rollout metadata + filtered content)
        │
5. Call model with JSON output schema constraint:
   {
     "raw_memory": "string",       // Detailed markdown
     "rollout_summary": "string",  // Compact summary
     "rollout_slug": "string|null" // Filesystem-safe slug
   }
        │
6. Post-process:
   - Redact secrets: replace tokens/keys with [REDACTED_SECRET]
   - No-op gate: empty output → "succeeded_no_output" (not all rollouts yield learnings)
        │
7. Store in SQLite stage1_outputs table
```

### Phase 1 System Prompt Highlights

The extraction model is instructed to:

**Apply a no-op gate**: Only save high-signal learnings. No-op is preferred over noise.

**Classify task outcomes**: success / partial / fail / uncertain

**Identify high-signal patterns**:
- Proven procedures (not just code, but sequence + rationale)
- Failure shields (what went wrong, why, how to avoid)
- Decision triggers (rules that produce predictable outcomes)
- Environmental facts (paths, versions, configs not in code)

**Output format** — `raw_memory`:
```markdown
---
description: Brief description
task: what the agent did
task_group: repo/project/workflow
task_outcome: success|partial|fail|uncertain
keywords: comma, separated, handles
---

### Task 1
- **User goal**: what the user wanted
- **What worked**: proven steps
- **What didn't work**: failure context
- **Validation**: how success was verified
- **Failure shields**: what to avoid next time
- **Evidence pointers**: file:line references
```

**Output format** — `rollout_summary`:
```markdown
One-line summary

## Rollout context
- cwd, tools used, model

## User preferences
- observed style/workflow choices

## Task 1
### Key steps
### Things that did not work
### Reusable knowledge
### References
```

---

## 5. Phase 2: Global Consolidation

### Constants

| Parameter | Value |
|---|---|
| Model | `gpt-5.3-codex` |
| Reasoning effort | Medium |
| Job lease | 3,600 seconds |
| Heartbeat interval | 90 seconds |
| Singleton | Only ONE phase-2 job runs globally |

### Input Selection Algorithm

```sql
-- Query top-N memories, ranked by usage and recency
SELECT * FROM stage1_outputs
WHERE (raw_memory != '' OR rollout_summary != '')
  AND memory_mode = 'enabled'
  AND (
    (last_usage IS NOT NULL AND last_usage >= now - max_unused_days)
    OR COALESCE(last_usage, source_updated_at) >= now - max_unused_days
  )
ORDER BY usage_count DESC,
         COALESCE(last_usage, source_updated_at) DESC,
         source_updated_at DESC
LIMIT 256
```

### Diff Computation

```
current = rank_top_256(stage1_outputs)
previous = stage1_outputs WHERE selected_for_phase2 = 1

retained = current ∩ previous    (by thread_id + snapshot match)
added    = current - previous    (new memories)
removed  = previous - current    (dropped memories)
```

### Consolidation Pipeline

```
1. Claim global job (singleton lock via jobs table)
   ├── SkippedNotDirty: no new phase-1 outputs → skip
   ├── SkippedRunning: another process owns lock → skip
   └── Claimed: proceed with input_watermark
        │
2. Get input selection (top-256 memories + diff)
        │
3. Sync filesystem artifacts:
   ├── Write raw_memories.md (merged, latest-first)
   ├── Write rollout_summaries/*.md (one per retained memory)
   ├── Prune old rollout_summaries not in retention set
   └── If no memories: clean up MEMORY.md, memory_summary.md, skills/
        │
4. Spawn consolidation sub-agent:
   ├── Config: read-only sandbox EXCEPT memory folder (workspace-write)
   ├── No network access
   ├── No tool approval (AskForApproval::Never)
   ├── No collaboration delegation
   ├── CWD = memory root folder
   ├── System prompt: consolidation.md (603 lines)
   └── User message: diff summary (added/retained/removed threads)
        │
5. Heartbeat loop: refresh job lease every 90s while agent runs
        │
6. On completion:
   ├── Mark job succeeded
   ├── Update watermark
   └── Set selected_for_phase2 = 1 on current selection rows
```

### Consolidation Modes

The consolidation agent operates in one of two modes:

**INIT** (first-time):
- Read all raw_memories.md
- Build MEMORY.md from scratch (task groups, keywords, learnings)
- Build memory_summary.md (user profile, tips, index)
- Optionally create skills/

**INCREMENTAL UPDATE** (subsequent runs):
- Read raw_memories.md + existing MEMORY.md + memory_summary.md
- Process diff: integrate added memories, prune removed ones
- Surgical updates: don't rewrite unchanged sections
- Stability: preserve working structure, only edit where diff applies

---

## 6. Memory Artifacts (Output Files)

### Directory Structure

```
${codex_home}/memories/
├── memory_summary.md              # Always loaded into prompt (≤5K tokens)
├── MEMORY.md                      # Searched on demand by agent
├── raw_memories.md                # Phase 1 input (merged, latest-first)
├── rollout_summaries/
│   ├── 2026-02-17T21-23-02-LN3m-weekly_report.md
│   ├── 2026-02-15T14-05-30-aBc3-auth_refactor.md
│   └── ...
└── skills/
    ├── deploy-to-staging/
    │   ├── SKILL.md
    │   ├── scripts/deploy.sh
    │   └── templates/config.yaml
    └── run-test-suite/
        └── SKILL.md
```

### `memory_summary.md` — Navigation Layer (Always Loaded)

```markdown
## User Profile
Vivid snapshot: roles, workflows, preferences (≤500 words)

## General Tips
Durable, actionable cross-project guidance

## What's in Memory

### Recent Active Memory Window (last 3 days)
- **2026-03-07**: Auth refactor — keywords: oauth, jwt, middleware
- **2026-03-05**: CI pipeline — keywords: github-actions, deploy
- **2026-03-03**: DB migration — keywords: sqlx, postgres, schema

### Older Memory Topics
- **Test infrastructure** — keywords: jest, vitest, coverage
- **API design** — keywords: rest, openapi, validation
```

### `MEMORY.md` — Knowledge Handbook (On-Demand)

```markdown
# Task Group: project-x/auth

scope: Authentication and authorization for project-x

## Task 1
### rollout_summary_files
- `rollout_summaries/2026-03-07-LN3m-auth_refactor.md`
  - cwd: /home/user/project-x
  - rollout_path: ~/.codex/sessions/2026/03/07/rollout-...jsonl
  - updated_at: 2026-03-07T21:23:02Z
  - thread_id: 019c6e27-e55b-73d1-87d8-...

### keywords
oauth, jwt, middleware, refresh-token, session

### learnings
- JWT refresh flow: must invalidate old token before issuing new one
- Middleware ordering matters: auth → rate-limit → validation
- Test with expired tokens: `scripts/gen-expired-jwt.sh`

## General Tips
- Always check token expiry before making API calls
- Use `just test-auth` to run auth-specific test suite
```

### `skills/` — Reusable Procedures

```markdown
# skills/deploy-to-staging/SKILL.md
---
name: deploy-to-staging
description: Deploy current branch to staging environment
---

## Steps
1. Run `scripts/deploy.sh --env staging`
2. Wait for health check: `curl https://staging.example.com/health`
3. Verify deployment: `just smoke-test-staging`

## Common Issues
- If health check fails, check `kubectl logs -f deploy/staging`
```

---

## 7. Memory Retrieval During Agent Execution

### Injection Pipeline

```
Turn starts
    │
build_memory_tool_developer_instructions(codex_home)
    │
    ├── Read ${codex_home}/memories/memory_summary.md
    ├── Truncate to 5,000 tokens
    ├── Render read_path.md template:
    │   └── {{ base_path }} = memory root
    │   └── {{ memory_summary }} = truncated summary
    └── Append as DeveloperInstructions to turn context
```

### read_path.md — Decision Boundary

The injected instructions tell the agent:

**When to use memory**:
- Task relates to workspace, repo, or prior decisions
- User references past work or preferences
- Complex setup that might have been done before

**When to skip**:
- Simple, self-contained tasks
- Questions unrelated to workspace history
- First interaction with a new project

**Quick memory pass** (5 steps):
1. Skim memory_summary.md (already in context)
2. Grep MEMORY.md for relevant keywords
3. Open 1-2 matching rollout_summaries
4. Apply any relevant learnings
5. If memory is stale, update it in the same turn

### Citation Format

When the agent reads and uses memory files:

```xml
<oai-mem-citation>
<citation_entries>
MEMORY.md:234-236|note=[auth middleware ordering]
rollout_summaries/2026-03-07-LN3m-auth_refactor.md:10-12|note=[JWT flow]
</citation_entries>
<rollout_ids>
019c6e27-e55b-73d1-87d8-4e01f1f75043
</rollout_ids>
</oai-mem-citation>
```

---

## 8. Usage Tracking

### How Citations Are Detected

```rust
// codex-rs/core/src/memories/usage.rs
// Parses agent tool calls (shell/read) for memory file paths:
// - MEMORY.md
// - memory_summary.md
// - raw_memories.md
// - rollout_summaries/*
// - skills/*
```

### What Gets Updated

```sql
UPDATE stage1_outputs
SET usage_count = usage_count + 1,
    last_usage = strftime('%s', 'now')
WHERE thread_id IN (cited_thread_ids);
```

This feedback loop ensures **frequently-cited memories rank higher** in
Phase 2 input selection, while unused memories eventually get pruned.

---

## 9. Memory Modes & Pollution

### Per-Thread Memory Mode

| Mode | Meaning |
|---|---|
| `enabled` | Default — eligible for extraction and usage |
| `disabled` | User explicitly turned off memory for this thread |
| `polluted` | Thread's memory caused a conflict → queued for forgetting |

### Pollution Flow

```
Agent detects memory caused incorrect behavior
    │
mark_thread_memory_mode_polluted(thread_id)
    │
    ├── Set memory_mode = 'polluted' in threads table
    ├── If thread was selected_for_phase2:
    │   └── Enqueue phase-2 forgetting (next consolidation removes it)
    └── Agent should clean up MEMORY.md references
```

---

## 10. Pruning & Retention

### Phase 1 Pruning (per-startup)

```sql
DELETE FROM stage1_outputs
WHERE selected_for_phase2 = 0                    -- Not in current baseline
  AND COALESCE(last_usage, source_updated_at)
      < (now - max_unused_days * 86400)          -- Stale
ORDER BY COALESCE(last_usage, source_updated_at) ASC  -- Stalest first
LIMIT 200;                                       -- Batch size
```

Keeps popular + recent memories. Removes forgotten/unused ones.

### Phase 2 Artifact Pruning

- Rollout summary files not in current retention set are deleted
- If no selected memories remain: MEMORY.md, memory_summary.md, and skills/ are cleaned up entirely

---

## 11. Database Schema

### stage1_outputs

```sql
CREATE TABLE stage1_outputs (
    thread_id TEXT PRIMARY KEY,
    source_updated_at INTEGER NOT NULL,
    raw_memory TEXT NOT NULL,
    rollout_summary TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    rollout_slug TEXT,
    usage_count INTEGER,
    last_usage INTEGER,
    selected_for_phase2 INTEGER NOT NULL DEFAULT 0,
    selected_for_phase2_source_updated_at INTEGER,
    FOREIGN KEY(thread_id) REFERENCES threads(id) ON DELETE CASCADE
);
```

### jobs (memory-related kinds)

| kind | job_key | Semantics |
|---|---|---|
| `memory_stage1` | `{thread_id}` | One per rollout extraction |
| `memory_consolidate_global` | `global` | Singleton consolidation |

---

## 12. Complete Lifecycle Flowchart

```
Session Starts
    │
    ▼
[Check: not ephemeral? feature enabled? not subagent? state DB?]
    │ ALL YES
    ▼
Spawn async startup task (non-blocking)
    │
    ├── Phase 1: Prune
    │   └── DELETE stale stage1_outputs (batch=200, >30d unused)
    │
    ├── Phase 1: Claim (scan 5K threads, claim ≤16)
    │   └── Filter: source ∈ {cli,ui,webui}, age ≤30d, idle ≥6h, stale
    │
    ├── Phase 1: Extract (8 concurrent)
    │   ├── Load rollout JSONL
    │   ├── Filter + truncate content
    │   ├── Call gpt-5.1-codex-mini (low reasoning)
    │   ├── Parse JSON → raw_memory + rollout_summary + rollout_slug
    │   ├── Redact secrets
    │   └── Store in stage1_outputs
    │
    ├── Phase 2: Claim global job (singleton)
    │   └── Skip if not dirty or already running
    │
    ├── Phase 2: Select inputs (top-256, ranked by usage + recency)
    │   └── Compute diff vs previous baseline
    │
    ├── Phase 2: Sync filesystem
    │   ├── Write raw_memories.md
    │   ├── Write rollout_summaries/*.md
    │   └── Prune stale files
    │
    └── Phase 2: Spawn consolidation agent
        ├── Config: workspace-write on memory folder only, no network
        ├── Prompt: consolidation.md (603 lines) + diff summary
        ├── Heartbeat lease every 90s
        └── Agent updates: MEMORY.md, memory_summary.md, skills/
    │
    ▼
Session Continues
    │
    ▼
Each Turn: inject read_path.md + memory_summary.md (≤5K tokens)
    │
    ▼
Agent reads memory files → usage_count incremented → informs next Phase 2
```

---

## 13. Failure Modes & Recovery

| Failure | Handling | Principle |
|---|---|---|
| Phase 1 model error | Retry 3x with 1h backoff | Bounded retry |
| Phase 1 empty output | Mark `succeeded_no_output` (not every rollout yields learnings) | No-op gate |
| Phase 2 agent crash | Heartbeat loss → mark failed, retry next startup | Lease expiry |
| Phase 2 watermark stale | Preserved → next run won't `skip_not_dirty` | Forward progress |
| Memory conflict | Mark thread `polluted` → next consolidation removes | Pollution tracking |
| Manual reset | `clear_memory_data()` or `reset_memory_data_for_fresh_start()` | Clean slate |

---

## 14. Key Design Principles

| Principle | Implementation |
|---|---|
| **Progressive disclosure** | summary (always loaded) → MEMORY.md (searched) → rollout_summaries (detailed) |
| **No-op gate** | Phase 1 only saves high-signal learnings; empty output is fine |
| **Usage-ranked retention** | `usage_count` + `last_usage` determine Phase 2 input priority |
| **Feedback loop** | Agent citations → usage tracking → Phase 2 ranking → better memories surface |
| **Pollution handling** | Conflicting memories marked and forgotten in next consolidation |
| **Secret redaction** | All memory outputs pass through `redact_secrets()` |
| **Singleton consolidation** | Only ONE Phase 2 job runs globally (prevents thrashing) |
| **Incremental updates** | Phase 2 diff computation enables surgical edits, not full rewrites |
| **Separation of extraction & consolidation** | Phase 1 is fast + parallel; Phase 2 is expensive + serial |

---

## Key Files

| File | Purpose |
|---|---|
| `core/src/memories/mod.rs` | Module root, constants, directory paths |
| `core/src/memories/start.rs` | Activation & startup orchestration |
| `core/src/memories/phase1.rs` | Phase 1 extraction pipeline |
| `core/src/memories/phase2.rs` | Phase 2 consolidation pipeline |
| `core/src/memories/prompts.rs` | Prompt building & template rendering |
| `core/src/memories/usage.rs` | Citation detection & usage tracking |
| `core/templates/memories/stage_one_system.md` | Phase 1 system prompt (337 lines) |
| `core/templates/memories/stage_one_input.md` | Phase 1 user message template |
| `core/templates/memories/consolidation.md` | Phase 2 system prompt (603 lines) |
| `core/templates/memories/read_path.md` | Agent-facing memory instructions (169 lines) |
| `state/src/lib.rs` | SQLite operations for stage1_outputs + jobs |
| `config/src/types.rs` | MemoriesConfig definition |
