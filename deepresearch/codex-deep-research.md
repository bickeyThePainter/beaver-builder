# Codex Deep Research

## Executive Summary

**Codex** is OpenAI's local AI coding agent — a production-grade, multi-platform system
that runs entirely on your machine. It combines a high-performance Rust core engine with
TypeScript SDK/CLI interfaces, advanced multi-layer sandboxing, an AI-powered approval
system (Guardian), and an extensible skills/MCP plugin architecture.

**Repo**: https://github.com/openai/codex
**License**: Apache-2.0
**Primary Languages**: Rust (~80%), TypeScript (~15%), Nix/Bazel (~5%)

---

## 1. Architecture Overview

```
┌───────────────────────────────────────────────────────────────┐
│                     USER INTERFACES                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │
│  │ codex-cli│  │   TUI    │  │app-server│  │  MCP Server  │ │
│  │  (Node)  │  │(ratatui) │  │(WebSocket│  │ (rmcp-based) │ │
│  │          │  │          │  │  + HTTP)  │  │              │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘ │
│       │              │             │               │         │
├───────┴──────────────┴─────────────┴───────────────┴─────────┤
│                     CORE ENGINE (codex-core)                  │
│  ┌────────────┐ ┌──────────┐ ┌────────────┐ ┌─────────────┐ │
│  │ Thread Mgr │ │ Skills   │ │  Guardian   │ │ MCP Conn Mgr│ │
│  │ & Context  │ │ System   │ │ (AI Approvals)│ │           │ │
│  └────────────┘ └──────────┘ └────────────┘ └─────────────┘ │
├──────────────────────────────────────────────────────────────┤
│                    PROTOCOL LAYER                             │
│  Types · Events · Approvals · Policies · MCP · Config        │
├──────────────────────────────────────────────────────────────┤
│                 SECURITY & SANDBOXING                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Seatbelt │  │ bwrap +  │  │ Windows  │  │ ExecPolicy  │ │
│  │ (macOS)  │  │ Landlock │  │ Sandbox  │  │ (Starlark)  │ │
│  │          │  │ (Linux)  │  │          │  │             │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────┘ │
├──────────────────────────────────────────────────────────────┤
│                    UTILITIES (20+ crates)                     │
│  pty · git · cache · image · file-search · shell-command ... │
└──────────────────────────────────────────────────────────────┘
```

### Monorepo Structure

```
codex/
├── codex-cli/              # Node.js CLI wrapper — platform binary dispatcher
├── codex-rs/               # Rust workspace (70+ crates)
│   ├── core/               # Business logic, agent loop, orchestration
│   ├── tui/                # Terminal UI (ratatui-based)
│   ├── cli/                # CLI binary & subcommands
│   ├── exec/               # Non-interactive execution (codex-exec)
│   ├── app-server/         # WebSocket backend for IDE extensions
│   ├── protocol/           # Shared types & protocol contracts
│   ├── config/             # TOML config parsing, profiles, schema
│   ├── skills/             # Skill loading & management
│   ├── execpolicy/         # Starlark-based execution policies
│   ├── linux-sandbox/      # bwrap + Landlock sandboxing
│   ├── windows-sandbox-rs/ # Windows-specific sandbox
│   ├── mcp-server/         # Model Context Protocol server
│   ├── rmcp-client/        # MCP client (OAuth2, HTTP/WS)
│   ├── guardian/           # AI-powered approval system
│   ├── apply-patch/        # Safe code patch application
│   ├── artifacts/          # Runtime artifact management
│   ├── backend-client/     # Codex backend HTTP client
│   ├── login/              # Auth flows (ChatGPT, API key, device code)
│   ├── otel/               # OpenTelemetry tracing & metrics
│   ├── state/              # Session persistence (SQLite)
│   └── utils/              # 20+ utility crates
├── sdk/typescript/         # @openai/codex-sdk (TypeScript SDK)
├── shell-tool-mcp/         # Patched Bash/Zsh for safe MCP execution
├── docs/                   # Contributing guides, installation docs
├── scripts/                # Build & release automation
├── MODULE.bazel            # Bazel build system config
├── Cargo.toml              # Rust workspace root
├── pnpm-workspace.yaml     # pnpm monorepo packages
├── justfile                # Task runner (replaces Make)
└── flake.nix               # Nix dev environment
```

---

## 2. Core Engine (`codex-core`)

The heart of Codex. Orchestrates the agent loop, managing conversation threads,
tool execution, skill injection, and approval workflows.

### Key Modules

| Module | Responsibility |
|---|---|
| `codex.rs` | Main `Codex` struct — turn orchestration, context management |
| `codex_thread.rs` | Thread state machine |
| `thread_manager.rs` | Conversation lifecycle management |
| `exec.rs` | Sandboxed command execution |
| `exec_policy.rs` | Policy evaluation before execution |
| `skills/` | Skill discovery, loading, injection, invocation |
| `guardian.rs` | AI-driven risk assessment for approvals |
| `mcp_connection_manager.rs` | MCP server lifecycle management |
| `memories/` | Agent memory system (phase1, phase2, citations) |
| `context_manager/` | Context aggregation & history management |
| `models_manager/` | Model provider info & selection |
| `sandboxing/` | Platform-specific sandbox orchestration |
| `config/` | Runtime configuration resolution |

### Agent Loop Data Flow

```
User Input (stdin / WebSocket / MCP)
  ↓
Thread Manager (creates/resumes thread)
  ↓
Context Manager (assembles context: history + skills + memories)
  ↓
OpenAI API (model inference — streaming response)
  ↓
Tool Call Routing:
  ├── Shell command  → ExecPolicy check → Sandbox → Execute
  ├── Apply-patch    → Approval workflow → Sandbox → Apply
  ├── MCP tool       → MCP Connection Mgr → External server
  ├── Web search     → Search provider
  └── Built-in tools → Direct handler
  ↓
Result Integration → Event Stream → UI Rendering
```

---

## 3. Protocol Layer (`codex-protocol`)

Defines all types for inter-component communication. Designed for minimal
dependencies and cross-language compatibility (auto-generates TypeScript types
via `ts-rs` and JSON schemas via `schemars`).

### Key Type Categories

| Category | Types | Purpose |
|---|---|---|
| **Operations** | `Op` | User-submitted operations (submission queue) |
| **Events** | `Event` | Agent responses (event queue) |
| **Items** | `TurnItem`, `ContentItem`, `ResponseItem` | Message/action modeling |
| **Policies** | `SandboxPolicy`, `FileSystemSandboxPolicy`, `NetworkSandboxPolicy` | Permission models |
| **Approvals** | `ExecApprovalRequestEvent`, `ApplyPatchApprovalRequestEvent` | Approval workflows |
| **MCP** | `Tool`, `Resource`, `CallToolResult` | Model Context Protocol |
| **Config** | `Settings`, `SandboxMode`, `ApprovalMode` | Configuration types |

### Sandbox Modes

| Mode | Filesystem | Network | Use Case |
|---|---|---|---|
| `read-only` | Full read, no write | Restricted | Safe exploration |
| `workspace-write` | Write to workspace (`.git` protected) | Restricted | Normal development |
| `danger-full-access` | Unrestricted | Enabled | Trusted environments |

---

## 4. Security & Sandboxing (Defense in Depth)

Codex implements **four layers** of security:

### Layer 1: OS-Level Sandboxing

**macOS — Seatbelt**:
- Native `sandbox-exec` with configurable profiles
- Filesystem access control via `SandboxPolicy`
- Permission extensions: `macos_preferences`, `macos_automation`, `macos_accessibility`
- `.git` directories automatically protected in `workspace-write` mode

**Linux — Bubblewrap + Landlock**:
- Read-only root filesystem by default
- Writable paths overlaid via `--bind`
- Protected subpaths (`.git`, `.codex`) re-mounted read-only
- Network namespace isolation with managed proxy routing
- Seccomp filter blocks unauthorized socket creation
- User/PID namespace isolation

**Windows — Custom Sandbox**:
- Helper process orchestration
- User isolation, firewall rules, token/identity management
- Registry and filesystem access controls

### Layer 2: Execution Policy Engine (Starlark DSL)

```python
# Example policy rule
prefix_rule(
    pattern=["rm", ["-rf", "-r"]],
    decision="forbidden",
    justification="Recursive delete not allowed"
)

prefix_rule(
    pattern=["cargo", "test"],
    decision="allow",
    justification="Unit tests are safe"
)
```

- **Decisions**: `allow`, `prompt`, `forbidden`
- **Patterns**: Ordered token arrays with alternatives (OR logic)
- **Host Executables**: Constrain which paths resolve through basename rules

### Layer 3: Permission & Approval System

| Policy | Behavior |
|---|---|
| `never` | Reject all risky operations |
| `on-request` | Prompt user for each risky action |
| `unless-trusted` | Allow for trusted projects, prompt otherwise |
| `on-failure` | Auto-approve unless previous failure |
| `guardian` | AI-driven approval (secondary model assesses risk) |

### Layer 4: Guardian AI Approval (MVP)

A secondary AI model reviews high-risk operations:
- Receives operation context + conversation history
- Assesses risk level: `low` / `medium` / `high`
- Provides evidence-based reasoning
- Used for: exec operations, apply-patch, network allowlist misses
- Maintains transcript budget for context preservation

---

## 5. Skills System

An autonomous tool/agent framework enabling Codex to perform complex, multi-step tasks.

### Architecture

```
SkillsManager (central coordinator)
  ├── loader.rs    → discovers skills from .codex/skills/ + remote
  ├── injection.rs → injects skill context into LLM prompts
  ├── remote.rs    → remote skill execution
  └── invocation_utils.rs → implicit/explicit skill mentions
```

### Skill Format

Skills are defined via `SKILL.md` frontmatter + template:

```markdown
---
name: babysit-pr
description: Monitor a PR through CI and review
---

# Instructions
...
```

### Example: `babysit-pr` Skill

Located at `.codex/skills/babysit-pr/` — a real-world skill that:
- Polls CI status and review comments
- Classifies test failures (flaky vs. real)
- Auto-fixes branch issues
- Uses `gh` CLI for GitHub operations
- Implements a state machine for continuous monitoring

---

## 6. MCP (Model Context Protocol) Integration

Codex is both an MCP **client** and **server**:

### As MCP Client (`rmcp-client`)
- Connects to external MCP servers for additional tools
- OAuth2 authentication support
- HTTP/WebSocket transports
- Keyring-based credential storage

### As MCP Server (`mcp-server`)
- Exposes Codex capabilities to other MCP consumers
- Bridges to external MCP servers
- Shell command execution via patched Bash/Zsh

### MCP Tool Types

| Type | Source | Example |
|---|---|---|
| Shell commands | Built-in | `bash`, `python` |
| Apply-patch | Built-in | Code modification |
| Web search | Built-in | Internet queries |
| MCP tools | External servers | Custom integrations |
| Skills | `.codex/skills/` | Complex workflows |

---

## 7. User Interfaces

### CLI (`codex-cli`)

Node.js wrapper that dispatches to platform-specific Rust binaries:
- Detects OS/arch → loads `@openai/codex-{platform}` binary
- Manages PATH for bundled tools
- Proxies signals (SIGINT, SIGTERM, SIGHUP)

### TUI (Terminal User Interface)

Built with **ratatui** framework:
- Real-time interactive agent interface
- Voice input support (optional)
- Image display (JPEG, PNG, GIF, WebP)
- Clipboard integration
- Snapshot testing with `insta`

### App Server (IDE Extensions)

WebSocket-based backend for rich clients (VS Code, Cursor, Windsurf):
- JSON-RPC 2.0 protocol (JSONL transport)
- Bidirectional communication
- Threads, conversations, messages, approvals, skills, artifacts
- v2 API (active development)

### TypeScript SDK (`@openai/codex-sdk`)

Programmatic access for building on top of Codex:
- `Codex` class → factory for threads
- `Thread` class → `run()` and `runStreamed()`
- JSONL event streaming via `AsyncGenerator<ThreadEvent>`
- Zod validation schemas + full TypeScript types

---

## 8. Build System & Development Workflow

### Build Tools

| Tool | Purpose | Config |
|---|---|---|
| **Cargo** | Rust compilation | `Cargo.toml` workspace |
| **Bazel** | Primary build system (with remote caching) | `MODULE.bazel`, `.bazelrc` |
| **pnpm** | Node.js monorepo management | `pnpm-workspace.yaml` |
| **just** | Task runner | `justfile` |
| **Nix** | Dev environment | `flake.nix` |

### Key Commands

```bash
just fmt              # Format code (Rust + TS)
just fix -p <crate>   # Lint fixes
just test             # Run nextest
just clippy           # Clippy linting
just codex            # Run CLI
just bazel-test       # Bazel build + test
```

### Toolchain Requirements

- **Rust**: Edition 2024, toolchain 1.93.0
- **Node.js**: >= 22
- **pnpm**: >= 10.29.3

---

## 9. CI/CD Pipeline

### GitHub Actions Workflows

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Push/PR | Prettier, README validation, npm staging |
| `rust-ci.yml` | Push/PR | Format, lint, build (10 configs), test (5 configs) |
| `bazel.yml` | Push/PR | Experimental Bazel build (4 configs) |
| `rust-release.yml` | Tag `rust-v*` | Multi-platform binary builds (6 targets) |
| `sdk.yml` | Push/PR | TypeScript SDK build + test |
| `shell-tool-mcp-ci.yml` | Push/PR | Shell tool format, test, build |

### Build Matrix (Rust CI)

| Platform | Architectures | Profiles |
|---|---|---|
| macOS | arm64, x86_64 | dev, release |
| Linux (gnu) | x86_64, arm64 | dev, release |
| Linux (musl) | x86_64 | dev, release |
| Windows | x86_64 | dev, release |

### Performance Optimizations

- **sccache**: Compiler cache with GitHub Actions backend
- **BuildBuddy**: Remote Bazel execution & cache
- **cargo-nextest**: Faster parallel test execution
- **Smart change detection**: Skip unnecessary jobs based on modified paths
- **Hermetic musl builds**: Zig toolchain with UBSan wrapper

---

## 10. Testing Strategy

### Rust Testing

- **Framework**: cargo test + cargo nextest
- **Snapshot testing**: `insta` crate (especially TUI)
- **Custom macros**: `#[large_stack_test]` for stack-heavy tests
- **CI profile**: `ci-test` with reduced debug info
- **Coverage**: 5 platform configurations

### TypeScript Testing

- **Framework**: Jest with ts-jest
- **Packages**: SDK tests (abort, exec, run, runStreamed), shell-tool-mcp tests
- **Coverage**: Available via `coverage` script

### Integration Testing

- DotSlash for cross-platform integration
- Node.js REPL integration tests
- Bubblewrap tests (Linux unprivileged user namespaces)

---

## 11. Distribution

### NPM Packages

| Package | Description |
|---|---|
| `@openai/codex` | Main CLI (dispatches to platform binary) |
| `@openai/codex-{platform}` | Platform-specific native binaries |
| `@openai/codex-sdk` | TypeScript SDK |
| `@openai/codex-shell-tool-mcp` | Patched shell binaries |
| `@openai/codex-responses-api-proxy` | API proxy binary |

### Platform Targets

- macOS: aarch64, x86_64
- Linux: x86_64/aarch64 (gnu and musl)
- Windows: x86_64, aarch64

### Installation

```bash
npm i -g @openai/codex
# or
brew install --cask codex
```

---

## 12. Recent Development Focus (Last 30 Commits)

| Theme | PRs | Summary |
|---|---|---|
| **Sandbox hardening** | #13453, #13452, #13451, #13449, #13448 | Split filesystem policies, denied path preservation, Seatbelt extensions |
| **Guardian approval MVP** | #13692 | AI-driven risk assessment for operation approval |
| **Task abort handling** | #13874 | Stabilized abort follow-up behavior |
| **Realtime features** | #13796, #13640 | Streaming TTY, audio frame handling |
| **MCP tool enhancements** | #13807 | Always-allow option for MCP tool calls |
| **TUI polish** | #13670, #13896 | Context window display, fast mode indicator |
| **Database simplification** | Various | Schema and state management cleanup |

---

## 13. Design Philosophy

1. **Defense in Depth** — Four layers of security (OS sandbox → policy engine → approvals → guardian AI)
2. **Principle of Least Privilege** — Default to `read-only`; explicit approval for escalation
3. **AI-Assisted Safety** — Guardian system uses secondary AI for risk assessment
4. **Protocol-First** — Core logic separated from UI via protocol layer; types auto-generated for TypeScript
5. **Extensibility** — Skills, MCP servers, custom tools, configurable policies
6. **Cross-Platform** — Same core logic, platform-specific security implementations
7. **Transparency** — Policies in code, approvals visible to user, event streams auditable

---

## 14. Key Takeaways

- **Codex is not just a CLI** — it's a full agent platform with TUI, IDE integration, SDK, and MCP support
- **Security is the defining feature** — 4-layer defense-in-depth with an AI guardian system
- **Rust dominates** — 70+ crates, ~80% of code, chosen for performance + memory safety
- **The protocol layer is the contract** — clean separation between core logic and all interfaces
- **Skills make it autonomous** — complex multi-step workflows defined declaratively
- **Active development** — heavy focus on sandbox hardening, approval systems, and platform parity
