use serde::{Deserialize, Serialize};
use super::pipeline::Stage;

/// Maps pipeline stages to agent configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub role: Stage,
    pub model: String,
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub tools: Vec<String>,
}

impl AgentConfig {
    /// Default agent configuration for each pipeline stage.
    ///
    /// Model assignments:
    /// - IntentClarifier: gpt-5.2-codex (fast, good at dialog)
    /// - Planner (designer): gpt-5.4 (strongest reasoning)
    /// - Coder: gpt-5.3 (good balance of speed and capability)
    /// - Reviewer: gpt-5.4 (strongest for arch/quality review)
    /// - Default: gpt-5.4
    pub fn for_stage(stage: Stage) -> Self {
        match stage {
            Stage::IntentClarifier => Self {
                role: stage,
                model: "gpt-5.2-codex".into(),
                system_prompt: INTENT_CLARIFIER_PROMPT.into(),
                temperature: 0.7,
                max_tokens: 4096,
                tools: vec![],
            },
            Stage::InitAgent => Self {
                role: stage,
                model: "gpt-5.3-codex".into(),
                system_prompt: INIT_AGENT_PROMPT.into(),
                temperature: 0.2,
                max_tokens: 2048,
                tools: vec!["create_file".into(), "create_dir".into(), "git_init".into()],
            },
            Stage::Planner => Self {
                role: stage,
                model: "gpt-5.4".into(),
                system_prompt: PLANNER_PROMPT.into(),
                temperature: 0.4,
                max_tokens: 8192,
                tools: vec!["read_file".into(), "list_dir".into(), "write_file".into()],
            },
            Stage::Coder => Self {
                role: stage,
                model: "gpt-5.3-codex".into(),
                system_prompt: CODER_PROMPT.into(),
                temperature: 0.3,
                max_tokens: 8192,
                tools: vec![
                    "read_file".into(), "write_file".into(),
                    "exec_command".into(), "grep".into(),
                ],
            },
            Stage::Reviewer => Self {
                role: stage,
                model: "gpt-5.4".into(),
                system_prompt: REVIEWER_PROMPT.into(),
                temperature: 0.2,
                max_tokens: 4096,
                tools: vec![
                    "read_file".into(), "list_dir".into(),
                    "grep".into(), "git_diff".into(),
                ],
            },
            // Deploy, Push, and terminal stages use default model
            _ => Self {
                role: stage,
                model: "gpt-5.4".into(),
                system_prompt: String::new(),
                temperature: 0.0,
                max_tokens: 1024,
                tools: vec![],
            },
        }
    }
}

// ---------------------------------------------------------------------------
// System prompt templates (abbreviated -- full versions loaded from files in prod)
// ---------------------------------------------------------------------------

const INTENT_CLARIFIER_PROMPT: &str = r#"You are the Intent Clarifier agent for Beaver Builder.
Your job is to have a focused dialog with the user to produce a clear, actionable specification.

Guidelines:
- Ask clarifying questions about scope, constraints, and success criteria
- Summarize the spec in structured form when you have enough information
- Output the final spec as JSON with fields: title, description, tech_stack, constraints, success_criteria
"#;

const INIT_AGENT_PROMPT: &str = r#"You are the Init Agent for Beaver Builder.
Your job is to scaffold the initial project structure based on the specification.

Guidelines:
- Create README.md, CHANGELOG.md, and a specs/ directory
- Set up the worktree with the correct branch
- Keep scaffolding minimal -- only create what's needed for the Planner
"#;

const PLANNER_PROMPT: &str = r#"You are the Planner agent for Beaver Builder.
Your job is to create a detailed implementation plan or design document.

Guidelines:
- Read the spec and any existing files in the workspace
- Produce a design-doc.md or implementation-plan.md
- Break the work into discrete, ordered tasks the Coder can execute
- Identify risks and open questions
"#;

const CODER_PROMPT: &str = r#"You are the Coder agent for Beaver Builder.
Your job is to implement the plan created by the Planner.

Guidelines:
- Follow the implementation plan step by step
- Write clean, well-tested code
- Use the available tools to read, write, and test files
- If you encounter ambiguity, make a reasonable choice and document it
"#;

const REVIEWER_PROMPT: &str = r#"You are the Reviewer agent for Beaver Builder.
Your job is to review the Coder's implementation for correctness and quality.

Guidelines:
- Check adherence to the spec and implementation plan
- Look for bugs, security issues, and architectural problems
- Output a structured verdict: { "verdict": "approved" | "request_changes", "issues": [...] }
- Be constructive -- provide specific, actionable feedback
"#;
