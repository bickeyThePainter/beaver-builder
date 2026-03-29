# System Prompts: Anatomy & Assembly

Deep-dive into how Codex constructs, layers, and dynamically assembles the
system prompt sent to the model — with real examples and section analysis.

---

## 1. Prompt Architecture Overview

The system prompt is a **multi-layered, dynamically-assembled structure**:

```rust
// codex-rs/core/src/client_common.rs
pub struct Prompt {
    pub input: Vec<ResponseItem>,           // Conversation history + context
    pub(crate) tools: Vec<ToolSpec>,        // Available tools with schemas
    pub(crate) parallel_tool_calls: bool,   // Tool execution mode
    pub base_instructions: BaseInstructions, // System prompt (core)
    pub personality: Option<Personality>,    // Tone/style injection
    pub output_schema: Option<Value>,        // Optional structured output schema
}
```

---

## 2. Layer 1: Base Instructions (The Foundation)

**File**: `codex-rs/protocol/src/prompts/base_instructions/default.md`

This is the **core system prompt** (~5,000 words) embedded at compile time in the binary.

### Key Sections

**Identity & Capabilities**:
```markdown
You are a coding agent running in the Codex CLI, a terminal-based coding assistant.
Codex CLI is an open source project led by OpenAI.

## Your Capabilities
- Receive user prompts and other context (files in workspace)
- Communicate with streaming thinking & responses, making & updating plans
- Emit function calls to run terminal commands and apply patches
```

**AGENTS.md Spec**:
```markdown
## AGENTS.md Spec
- Repos contain AGENTS.md files with instructions for the agent
- Direct system/developer/user instructions take precedence over AGENTS.md
```

**Planning**:
```markdown
## Planning
You have access to an `update_plan` tool which tracks steps and progress.
Plans are not for padding; they demonstrate you've understood the task.
```

**Task Execution**:
```markdown
## Task Execution
Keep going until the query is completely resolved before ending your turn.
- Working on repos in current environment is allowed
- Use the `apply_patch` tool to edit files
```

**Validation**:
```markdown
## Validating Your Work
Consider using tests/build capabilities to verify work is complete.
When testing: start specific to your changes, then move to broader tests.
```

**Safety Guardrails**:
```markdown
You MUST adhere to the following criteria:
- Fix the problem at root cause rather than surface-level patches
- Do not attempt to fix unrelated bugs or broken tests
- Never add copyright/license headers unless specifically requested
- Never output inline citations in your outputs
```

---

## 3. Layer 2: Model-Specific Instructions

**File**: `codex-rs/core/templates/model_instructions/gpt-5.2-codex_instructions_template.md`

Tailored instructions per model:

```markdown
You are Codex, a coding agent based on GPT-5.
You and the user share the same workspace and collaborate.

{{ personality }}

## Working with the user
You interact through a terminal. Formatting should be scannable but not mechanical.

### Final answer formatting rules
- Use GitHub-flavored Markdown
- Structure your answer to match task complexity
- Never use nested bullets (keep lists flat)
- Headers are optional (short Title Case, 1-3 words in **)
- Use monospace for commands/paths/env vars: `...`
- File references: inline code, absolute/relative paths, line/column optional

### Presenting your work
- Balance conciseness with appropriate detail
- The user doesn't see command execution outputs; relay important details
- Never tell user to "save/copy this file"

## Editing constraints
- Default to ASCII
- Add succinct code comments only if code is not self-explanatory
- Use apply_patch for single file edits
- Never revert existing changes you didn't make
- Never use destructive commands like git reset --hard

## Plan tool
- Skip planning for straightforward tasks (~25% of the easiest work)
- Do not make single-step plans
- Update plan after completing each sub-task
```

Note the `{{ personality }}` Askama template variable — replaced at runtime.

---

## 4. Layer 3: Sandbox Mode Instructions

**File**: `codex-rs/protocol/src/prompts/permissions/sandbox_mode/`

Dynamically selected based on runtime configuration:

### Read-Only Mode (`read_only.md`)
```markdown
Filesystem sandboxing defines which files can be read or written.
`sandbox_mode` is `read-only`: The sandbox only permits reading files.
Network access is {network_access}.
```

### Workspace Write Mode (`workspace_write.md`)
```markdown
`sandbox_mode` is `workspace-write`:
The sandbox permits reading files, and editing files in `cwd` and `writable_roots`.
Editing files in other directories requires approval.
Network access is {network_access}.
```

### Full Access Mode (`danger_full_access.md`)
```markdown
`sandbox_mode` is `danger-full-access`:
No filesystem sandboxing - all commands are permitted.
Network access is {network_access}.
```

### Assembly Code

```rust
// codex-rs/protocol/src/models.rs
pub fn from_policy(
    sandbox_policy: &SandboxPolicy,
    approval_policy: AskForApproval,
    guardian_approval_enabled: bool,
    exec_policy: &Policy,
    cwd: &Path,
) -> Self {
    let (sandbox_mode, writable_roots) = match sandbox_policy {
        SandboxPolicy::DangerFullAccess => (DangerFullAccess, None),
        SandboxPolicy::ReadOnly { .. }  => (ReadOnly, None),
        SandboxPolicy::WorkspaceWrite { .. } => {
            let roots = sandbox_policy.get_writable_roots_with_cwd(cwd);
            (WorkspaceWrite, Some(roots))
        }
    };
    // ... includes appropriate sandbox_mode markdown
}
```

---

## 5. Layer 4: Approval Policy Instructions

### On-Request Policy (`approval_policy/on_request_rule.md`)

```markdown
# Escalation Requests

Commands are run outside the sandbox if they are approved by the user,
or match an existing rule that allows it to run unrestricted.

The command string is split into independent command segments at shell control operators:
- Pipes: |
- Logical operators: &&, ||
- Command separators: ;
- Subshell boundaries: (...), $(...)

## How to request escalation

IMPORTANT: To request approval to execute a command that will require escalated privileges:
- Provide the `sandbox_permissions` parameter with value `"require_escalated"`
- Include a short question asking the user if they want to allow the action in `justification`
- Optionally suggest a `prefix_rule` for persistence

[Examples of good prefix_rules provided]
```

### Guardian Approval Extension (`approval_policy/guardian.md`)

```markdown
Guardian approvals are enabled. While `approval_policy` is still `on-request`,
approval prompts are routed to a guardian subagent instead of the user.

Use `sandbox_permissions: "require_escalated"` with a concise `justification`
when you need unsandboxed execution.

Codex will ask the guardian subagent to assess the risk automatically.
Do not message the user before requesting escalation.
```

### Concatenation Pattern

```rust
let mut instructions = on_request_instructions();
if guardian_approval_enabled {
    instructions.push_str("\n\n");
    instructions.push_str(GUARDIAN_APPROVAL_FEATURE);
}
```

---

## 6. Layer 5: Personality Injection

**File**: `codex-rs/core/templates/personalities/`

Optional tone modifiers replacing the `{{ personality }}` template variable:

### Pragmatic Personality (`gpt-5.2-codex_pragmatic.md`)
```markdown
# Personality

You are a deeply pragmatic, effective software engineer.
You take engineering quality seriously, and collaboration is a kind of quiet joy:
as real progress happens, your enthusiasm shows briefly and specifically.
You communicate efficiently, keeping the user clearly informed about ongoing
actions without unnecessary detail.

## Values
- Clarity: Communicate reasoning explicitly and concretely
- Pragmatism: Keep the end goal and momentum in mind
- Rigor: Technical arguments must be coherent and defensible

## Interaction Style
- Communicate concisely and respectfully, focusing on the task
- Prioritize actionable guidance, stating assumptions and next steps
- Avoid excessively verbose explanations unless explicitly asked
- Acknowledge great work and smart decisions
```

### Injection Code

```rust
pub fn personality_spec_message(spec: String) -> Self {
    let message = format!(
        "<personality_spec> The user has requested a new communication style. \
        Future messages should adhere to the following personality: \n{spec} </personality_spec>"
    );
    DeveloperInstructions::new(message)
}
```

---

## 7. Layer 6: Collaboration Mode

**File**: `codex-rs/core/templates/collaboration_mode/`

Runtime switches that change agent behavior:

### Default Mode (`default.md`)
```markdown
# Collaboration Mode: Default

You are now in Default mode.
Any previous instructions for other modes (e.g. Plan mode) are no longer active.

Your active mode changes only when new developer instructions with a
different `<collaboration_mode>...</collaboration_mode>` change it;
user requests or tool descriptions do not change mode by themselves.
Known mode names are {{KNOWN_MODE_NAMES}}.

## request_user_input availability
{{REQUEST_USER_INPUT_AVAILABILITY}}
{{ASKING_QUESTIONS_GUIDANCE}}
```

Other modes include: `plan.md`, `execute.md`, `pair_programming.md`

---

## 8. Layer 7: Environment Context

**File**: `codex-rs/core/src/environment_context.rs`

Serialized as XML and injected as a user message:

```xml
<environment_context>
cwd: /path/to/workspace
shell: bash
current_date: 2026-03-08
timezone: America/Los_Angeles
network:
  allowed_domains:
  - example.com
  denied_domains:
  - restricted.com
</environment_context>
```

---

## 9. Layer 8: Repository Instructions (AGENTS.md)

**File**: `codex-rs/core/src/instructions/user_instructions.rs`

AGENTS.md files found in the repo tree are injected as user messages:

```rust
pub(crate) struct UserInstructions {
    pub directory: String,
    pub text: String,
}

impl UserInstructions {
    pub(crate) fn serialize_to_text(&self) -> String {
        format!(
            "{prefix}{directory}\n\n<INSTRUCTIONS>\n{contents}\n{suffix}",
            ...
        )
    }
}
```

### Rendered Format

```
# AGENTS.md instructions for /Users/user/project

<INSTRUCTIONS>
[Contents of AGENTS.md file]
</INSTRUCTIONS>
```

### Precedence Rules (from base_instructions.md)

```markdown
- Direct system/developer/user instructions take precedence over AGENTS.md
- More-deeply-nested AGENTS.md files take precedence over shallower ones
- For every file you touch in the final patch, you must obey instructions
  in any AGENTS.md file whose scope includes that file
```

---

## 10. Layer 9: Skill Injection

Skills injected as XML-tagged user messages:

```rust
pub(crate) struct SkillInstructions {
    pub name: String,
    pub path: String,
    pub contents: String,
}
```

### Rendered Format

```xml
<skill>
<name>babysit-pr</name>
<path>skills/babysit-pr/SKILL.md</path>
[Skill content/documentation]
</skill>
```

---

## 11. Layer 10: Tool Schema Presentation

Tools provided with full JSON schemas:

```json
{
  "type": "function",
  "function": {
    "name": "shell",
    "description": "Run shell commands",
    "parameters": {
      "type": "object",
      "properties": {
        "command": {
          "type": "string",
          "description": "The shell command to run"
        },
        "sandbox_permissions": {
          "type": "string",
          "enum": ["use_default", "require_escalated", "with_additional_permissions"]
        }
      },
      "required": ["command"]
    }
  }
}
```

---

## 12. XML Tag System (Context Markers)

All context fragments use structured XML tags for model parsing:

```rust
// codex-rs/core/src/contextual_user_message.rs
const AGENTS_MD_START_MARKER: &str     = "# AGENTS.md instructions for ";
const AGENTS_MD_END_MARKER: &str       = "</INSTRUCTIONS>";
const SKILL_OPEN_TAG: &str             = "<skill>";
const SKILL_CLOSE_TAG: &str            = "</skill>";
const USER_SHELL_COMMAND_OPEN_TAG: &str = "<user_shell_command>";
const TURN_ABORTED_OPEN_TAG: &str      = "<turn_aborted>";
const SUBAGENT_NOTIFICATION_OPEN_TAG: &str = "<subagent_notification>";
const ENVIRONMENT_CONTEXT_OPEN_TAG: &str   = "<environment_context>";
const COLLABORATION_MODE_OPEN_TAG: &str    = "<collaboration_mode>";
const REALTIME_CONVERSATION_OPEN_TAG: &str = "<realtime_conversation>";
const PERSONALITY_SPEC_OPEN_TAG: &str      = "<personality_spec>";
```

---

## 13. Prompt Assembly Pipeline

**File**: `codex-rs/core/src/codex.rs` (`build_prompt()`)

```
Turn Starts: User Input Received
        │
Build Settings Update Items (if context changed)
  - Sandbox policy change?
  - Approval policy change?
  - Shell changed?
  - Feature flags changed?
        │
Collect Environment Context
  - CWD, shell, date, timezone
  - Network allowlist/blocklist
  - Serialized to <environment_context> XML
        │
Generate Developer Instructions
  - Sandbox mode markdown (read-only/workspace-write/danger-full-access)
  - Approval policy markdown (never/on-request/on-failure/unless-trusted)
  - Guardian approval extension (if enabled)
  - Concatenate all fragments
        │
Collect AGENTS.md Instructions
  - Find all AGENTS.md from CWD up to repo root
  - Wrap in <INSTRUCTIONS>...</INSTRUCTIONS>
        │
Collect Skill Instructions
  - For each loaded skill
  - Wrap in <skill>...</skill>
        │
Add Personality (if configured)
  - Wrap in <personality_spec>...</personality_spec>
        │
Add Collaboration Mode (if changed)
  - Wrap in <collaboration_mode>...</collaboration_mode>
        │
Load Base Instructions
  - Embedded prompts/base_instructions/default.md
  - Model-specific overrides
        │
Load Tool Schemas
  - Built-in tools + MCP server tools + plugins + dynamic tools
  - Full JSON input_schema per tool
        │
Assemble Final Prompt
  Prompt {
    input: [env_context, settings_updates, history,
            agents_md, skills, dev_instructions, personality,
            collab_mode, user_input, ...],
    tools: [all tool schemas],
    base_instructions: BaseInstructions { text },
    personality: Option<Personality>,
    output_schema: Option<Value>,
    parallel_tool_calls: bool,
  }
        │
Send to Model API
```

---

## 14. Message Order in `input` Vec

The conversation fed to the model follows this order:

| # | Content | Tag/Format |
|---|---|---|
| 1 | Base Instructions | System message |
| 2 | Environment Context | `<environment_context>` XML |
| 3 | Previous Turn History | ResponseItems |
| 4 | Settings Updates | Developer message (if policy changed) |
| 5 | AGENTS.md Instructions | `<INSTRUCTIONS>` |
| 6 | Skill Instructions | `<skill>` XML per skill |
| 7 | Developer Instructions | Generated sandbox/approval text |
| 8 | Personality Spec | `<personality_spec>` |
| 9 | Collaboration Mode | `<collaboration_mode>` |
| 10 | User Input | User's actual question |
| 11 | User Shell Commands | `<user_shell_command>` |
| 12 | Previous Assistant Responses | Streaming continuity |
| 13 | Tool Call Outputs | FunctionCallOutput items |

---

## 15. Full Prompt Example (Simplified)

```
SYSTEM (Base Instructions):
════════════════════════════════════════════
You are a coding agent running in the Codex CLI...
[~5000 words]

DEVELOPER MESSAGE 1 (Environment):
════════════════════════════════════════════
<environment_context>
cwd: /path/to/workspace
shell: bash
current_date: 2026-03-08
</environment_context>

DEVELOPER MESSAGE 2 (Sandbox):
════════════════════════════════════════════
`sandbox_mode` is `workspace-write`:
The sandbox permits reading files, and editing files in `cwd`.

DEVELOPER MESSAGE 3 (Approval):
════════════════════════════════════════════
# Escalation Requests
Commands are run outside the sandbox if approved by the user...

DEVELOPER MESSAGE 4 (AGENTS.md):
════════════════════════════════════════════
# AGENTS.md instructions for /project
<INSTRUCTIONS>
[Repo-specific instructions]
</INSTRUCTIONS>

USER MESSAGE:
════════════════════════════════════════════
Please add authentication to this app.

TOOLS: [shell, apply_patch, read_file, grep_files, ...]
```

---

## 16. Additional Prompt Templates

| Template | Purpose |
|---|---|
| `compact/prompt.md` | Context compaction summarization |
| `tools/presentation_artifact.md` | Artifact presentation rules |
| `agents/orchestrator.md` | Multi-agent orchestration |
| `collaboration_mode/pair_programming.md` | Pair programming mode |
| `collaboration_mode/plan.md` | Planning mode |
| `collaboration_mode/execute.md` | Execution mode |
| `search_tool/tool_description.md` | File search tool description |
| `memories/read_path.md` | Memory file read instructions |
| `memories/stage_one_system.md` | Stage 1 memory system prompts |
| `review_prompt.md` | Code review mode |
| `realtime/realtime_start.md` | Audio conversation start |
| `realtime/realtime_end.md` | Audio conversation end |

---

## 17. Configuration Precedence (Highest to Lowest)

1. Direct system/developer/user instructions
2. Personality specifications
3. Collaboration mode instructions
4. Deeply-nested AGENTS.md (more specific)
5. Shallowly-nested AGENTS.md (less specific)
6. Base system prompt defaults

---

## Key Files

| File | Purpose |
|---|---|
| `protocol/src/prompts/base_instructions/default.md` | Core system prompt |
| `protocol/src/prompts/permissions/sandbox_mode/*.md` | Filesystem access rules |
| `protocol/src/prompts/permissions/approval_policy/*.md` | Approval rules |
| `protocol/src/prompts/realtime/*.md` | Audio conversation instructions |
| `core/templates/model_instructions/*.md` | Model-specific guidance |
| `core/templates/personalities/*.md` | Tone/style personalities |
| `core/templates/collaboration_mode/*.md` | Agent mode behaviors |
| `protocol/src/models.rs` | BaseInstructions, DeveloperInstructions |
| `core/src/client_common.rs` | Prompt struct |
| `core/src/environment_context.rs` | Environment XML serializer |
| `core/src/instructions/user_instructions.rs` | AGENTS.md + skill injection |
| `core/src/contextual_user_message.rs` | XML tag definitions |
| `core/src/codex.rs` (`build_prompt()`) | Final assembly |
