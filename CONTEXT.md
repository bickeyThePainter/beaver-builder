# Beaver Builder — Shared Agent Context

## Project Brief
AI coding agent harness that orchestrates multiple LLM agents in a pipeline workflow.
The pipeline supports back-and-forth between stages (e.g., Coder ↔ Reviewer loop).
Inspired by OpenAI Codex's architecture (SQ/EQ pattern, ToolHandler trait).

## Tech Stack
- **Backend**: Rust (axum, tokio, serde, chrono, uuid, tower-http, reqwest)
- **Frontend**: React 19 + Vite + TailwindCSS + Zustand + lucide-react
- **Package managers**: `cargo` (Rust), `bun` (JS/TS)
- **LLM Provider**: OpenAI (gpt-5.x models)

## Model Assignments
| Stage | Model |
|-------|-------|
| Intent Clarifier | gpt-5.2-codex |
| Init Agent | gpt-5.3-codex |
| Planner | gpt-5.4 |
| Coder | gpt-5.3-codex |
| Reviewer | gpt-5.4 |
| Default | gpt-5.4 |

## Environment
- OS: macOS (Darwin 25.3.0)
- `OPENAI_API_KEY`: set in env (164 chars)
- Backend server: port 3001
- Frontend dev server: port 5173
- HTTP proxy at 127.0.0.1:7892 (may affect WebSocket — use direct connections)

## Available Tools (for Tester)
Chrome DevTools MCP tools are available for browser E2E testing:
- `navigate_page`: go to a URL
- `take_snapshot`: get page a11y tree (element UIDs for interaction)
- `click`, `fill`, `type_text`: interact with elements by UID
- `take_screenshot`: capture visual evidence
- `evaluate_script`: run JS in browser (e.g., send WebSocket ops)
- `list_console_messages`: check for errors/warnings
- `list_network_requests`: inspect network traffic (filter by `websocket`)
- `wait_for`: wait for text to appear on page
- `lighthouse_audit`: accessibility/best-practices audit

## Reference Materials
- `gemini-web-page.html`: UI design reference (dark theme, indigo accent, 3 views)
- `deepresearch/`: Codex architecture deep research (agent loop, tool design, etc.)

## Anti-Patterns (Do NOT)
- Do not create static mockups — all UI interactions must go through WebSocket
- Do not use `format!("{:?}")` for serialization — use serde
- Do not hardcode model names in multiple places — use AgentConfig::for_stage()
- Do not skip LLM integration — UserMessage must actually call OpenAI API
- Do not create duplicate test data — Tester owns fixtures, Coder does not create test data
- Do not use `unwrap()` in production code — use proper Result types

## Acceptance Criteria (Gates)
- `cargo build` must succeed (warnings OK for dead_code)
- `cargo test` must pass all tests
- `cargo clippy` must have no lint errors (dead_code warnings OK)
- `bun run build` must succeed with no errors
- E2E: 3 scenarios (Happy Path, Review Loop, Human Rejection) must pass in real browser
