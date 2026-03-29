# Configuration & Extensibility — Codex Deep Research

## Why This Matters

An agent that only works in one environment is a demo. An agent that adapts to different
users, repos, teams, and security postures is a product. Codex's configuration system
is how it bridges this gap — without code changes.

---

## 7-Layer Configuration Stack

**File**: `core/src/config_loader/mod.rs:80-105`

Codex loads configuration from 7 sources in strict precedence order:

```
Highest priority (wins on conflict)
  ┌─ 1. Cloud        — Managed cloud requirements
  │  2. Admin        — Device management profiles (macOS MDM, etc.)
  │  3. System       — /etc/codex/requirements.toml (Unix)
  │  4. User         — ~/.codex/config.toml
  │  5. Tree         — Parent directories up to root (.codex/config.toml)
  │  6. Repo         — $(git root)/.codex/config.toml
  └─ 7. Runtime      — CLI flags, model selector, UI overrides
Lowest priority
```

### Trust Model

Not all config sources are equally trusted:

- **Layers 1-4** (Cloud, Admin, System, User): Always trusted
- **Layers 5-7** (Tree, Repo, Runtime): Only trusted if in a trusted directory

Trust is determined by `resolve_root_git_project_for_trust()`. Untrusted config files
are loaded but **disabled** — they exist but don't take effect.

### Merge Behavior

- Regular config values: later layers override earlier ones
- **Constraints/requirements**: first-defined wins (cannot be loosened by lower layers)
- Nested structures: deep merge with type-specific logic (e.g., `WebSearchLocation::merge()`)

This means an admin can enforce security constraints that no user or repo config can override.

---

## Skills System

### Definition Format (SKILL.md)

```markdown
---
name: babysit-pr
description: Monitor a PR through CI and review
---

# Instructions

1. Check PR status with `gh pr view`
2. If CI fails, read the logs and fix the issue
3. Re-push and repeat until CI passes
...
```

### Architecture

**File**: `core/src/config/types.rs:787+`

```rust
pub struct SkillsConfig {
    pub skills: Vec<SkillConfig>,
}

pub struct SkillConfig {
    pub path: AbsolutePathBuf,
    pub enabled: bool,
}
```

**Components**:
- **SkillsManager** — central coordinator
- **loader.rs** — discovers skills from `.codex/skills/` + remote sources
- **injection.rs** — injects skill context into LLM prompts
- **invocation_utils.rs** — handles implicit/explicit skill mentions

### Invocation

Skills can be invoked:
- **Explicitly**: User types `/babysit-pr` in the CLI
- **Implicitly**: Model recognizes skill description matches the task

When injected, the skill's instructions become part of the system prompt.

---

## Personality System

### Templates

**Directory**: `core/templates/personalities/`

Two built-in personalities:

| Personality | File | Tone |
|------------|------|------|
| `Friendly` | `gpt-5.2-codex_friendly.md` | Supportive, warm, team morale-focused |
| `Pragmatic` | `gpt-5.2-codex_pragmatic.md` | Direct, effective engineer, no fluff |
| `None` | (default) | Standard model behavior |

### How It Works

**File**: `core/src/models_manager/model_info.rs`

Model instructions templates use `{{ personality }}` placeholders:

```
ModelInstructionsVariables {
    personality_default: "",
    personality_friendly: LOCAL_FRIENDLY_TEMPLATE,
    personality_pragmatic: LOCAL_PRAGMATIC_TEMPLATE,
}
```

The personality template is injected into the model's base instructions when enabled
via `Feature::Personality`. Config-level overrides disable all personality templates.

---

## Collaboration Modes

### Modes

**File**: `protocol/src/config_types.rs:293-320`

| Mode | User-visible | Behavior |
|------|-------------|----------|
| `Plan` | Yes | Conversational planning, non-mutating actions only |
| `Default` | Yes | Standard execution |
| `PairProgramming` | No (internal) | Explain reasoning, ask for alignment |
| `Execute` | No (legacy) | — |

### Mode Structure

```rust
pub struct CollaborationMode {
    pub mode: ModeKind,
    pub settings: Settings,
}

pub struct Settings {
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub developer_instructions: Option<String>,
}
```

### Plan Mode: 3-Phase Planning

**Template**: `core/templates/collaboration_mode/plan.md`

1. **Phase 1**: Ground in environment (explore first, ask second)
2. **Phase 2**: Intent chat (what they actually want)
3. **Phase 3**: Implementation chat (decision-complete spec)

Plan mode output wrapped in `<proposed_plan>` for client rendering.

**Strict rules**: Only non-mutating actions (reads, searches). No file writes, no shell
commands that modify state. The model must plan before acting.

### Mode Switching

Modes are switched only by developer instructions via `<collaboration_mode>` XML tags.
User requests or tool descriptions do NOT change mode. Once switched, persists until
next explicit change.

---

## Feature Flags & Rollout

### Lifecycle Stages

**File**: `core/src/features.rs`

```rust
pub enum Stage {
    UnderDevelopment,          // Not for external use
    Experimental {             // Via /experimental menu
        name, menu_description, announcement
    },
    Stable,                    // Defaults on, flag kept for toggling
    Deprecated,                // Should not be used
    Removed,                   // Backward compat only
}
```

### Feature Registry

Each feature is registered with:
- `key: &'static str` — config key (e.g., `"guardian_approval"`)
- `stage: Stage` — lifecycle
- `default_enabled: bool` — initial state

**Examples**:
| Feature | Stage | Default | Purpose |
|---------|-------|---------|---------|
| `GuardianApproval` | Experimental | Off | AI-based approval review |
| `Personality` | Experimental | Off | Personality selection in TUI |
| `ChildAgentsMd` | Stable | On | AGENTS.md guidance injection |
| `ShellTool` | Stable | On | Shell command execution |
| `FastMode` | Stable | Off | Faster output mode |

### Loading

```toml
# ~/.codex/config.toml
[features]
guardian_approval = true
personality = true
```

Features loaded from `[features]` table, applied via `Features::with_defaults()`.
`FeatureOverrides` allow inline overrides for specific turns.

---

## AGENTS.md / Project Documentation

### How It Works

When `Feature::ChildAgentsMd` is enabled:
- Codex looks for AGENTS.md files in the project
- Content is injected into the system prompt as developer instructions
- Scope and precedence rules are appended automatically

### External Agent Config Migration

**File**: `core/src/external_agent_config.rs`

Codex detects configuration from other agents (Claude, etc.) via
`ExternalAgentConfigDetectOptions` and can migrate settings.

---

## MCP Server Configuration

### Transport Types

**File**: `core/src/config/types.rs:64-273`

Two transport modes:

**Stdio** (direct process):
```toml
[mcp_servers.my_server]
transport = "stdio"
command = "python"
args = ["-m", "my_mcp_server"]
cwd = "/path/to/workdir"
env = { "KEY" = "value" }
```

**StreamableHttp** (HTTP/WebSocket):
```toml
[mcp_servers.my_server]
transport = "streamable_http"
url = "https://api.example.com/mcp"
bearer_token_env_var = "MCP_TOKEN"
http_headers = { "Custom-Header" = "value" }
```

### Tool Filtering

```toml
[mcp_servers.my_server]
enabled_tools = ["tool_a", "tool_b"]    # Allow-list (only these available)
disabled_tools = ["tool_c"]             # Deny-list (these removed)
```

### Timeouts

```toml
startup_timeout_sec = 30    # Wait for server initialization
tool_timeout_sec = 60       # Default per-tool timeout
```

### Built-in: codex_apps

A special MCP server for ChatGPT connectors:
- Configurable gateway (legacy vs new)
- Bearer token from `CODEX_CONNECTORS_TOKEN`
- Endpoint: `https://api.openai.com/v1/connectors/gateways/flat/mcp`

---

## Environment Adaptation

### What's Gathered at Startup

- Current working directory
- Git root detection (for `.codex/config.toml` location)
- Trust level determination
- Config layer sources for each file loaded
- Project root markers (default: `[".git"]`)

### Environment Context Injection

The system injects environment information as XML into the system prompt:

```xml
<environment>
  <os>darwin</os>
  <shell>zsh</shell>
  <cwd>/Users/user/project</cwd>
  <git_root>/Users/user/project</git_root>
  <git_branch>main</git_branch>
  ...
</environment>
```

This gives the model awareness of its operating context without hardcoding assumptions.

---

## Design Insights

1. **Trust enforcement is non-negotiable.** Repo-level config can't override admin
   constraints. This prevents a malicious repo from weakening security.

2. **7 layers is not over-engineering.** Each layer serves a distinct stakeholder:
   cloud (org policy), admin (device policy), system (machine policy), user (personal
   preference), tree/repo (project convention), runtime (immediate override).

3. **Skills are the right abstraction for complex workflows.** Rather than hardcoding
   workflows, skills are discoverable, user-extensible, and described in natural language.
   The model can decide when a skill is relevant.

4. **Collaboration modes separate planning from execution.** Plan mode's strict "no
   mutations" rule forces the model to think before acting. This is a design constraint
   that improves output quality.

5. **Feature flags with lifecycle stages** prevent the "permanent experiment" problem.
   Features must progress through stages or be removed — they can't linger indefinitely
   in experimental state.

---

## Key Files

| Component | Path |
|-----------|------|
| Config loader | `core/src/config_loader/mod.rs` |
| Config types | `protocol/src/config_types.rs` |
| Skills | `core/src/config/types.rs:787+` |
| Personalities | `core/templates/personalities/` |
| Collaboration modes | `core/templates/collaboration_mode/` |
| Features | `core/src/features.rs` |
| MCP config | `core/src/config/types.rs:64-273` |
| External agent config | `core/src/external_agent_config.rs` |
| Config API | `app-server/src/config_api.rs` |
