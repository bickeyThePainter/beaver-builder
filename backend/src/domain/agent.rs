use super::pipeline::Stage;

/// Per-stage LLM configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: &'static str,
    pub system_prompt: &'static str,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl AgentConfig {
    /// Returns the agent configuration for the given pipeline stage.
    pub fn for_stage(stage: Stage) -> Self {
        match stage {
            Stage::Planner => Self {
                model: "gpt-5.4",
                system_prompt: PLANNER_PROMPT,
                temperature: 0.4,
                max_tokens: 4096,
            },
            Stage::InitAgent => Self {
                model: "gpt-5.3-codex",
                system_prompt: INIT_AGENT_PROMPT,
                temperature: 0.2,
                max_tokens: 4096,
            },
            Stage::Coder => Self {
                model: "gpt-5.3-codex",
                system_prompt: CODER_PROMPT,
                temperature: 0.3,
                max_tokens: 8192,
            },
            Stage::Reviewer => Self {
                model: "gpt-5.4",
                system_prompt: REVIEWER_PROMPT,
                temperature: 0.2,
                max_tokens: 4096,
            },
            _ => Self {
                model: "gpt-5.4",
                system_prompt: DEFAULT_PROMPT,
                temperature: 0.0,
                max_tokens: 4096,
            },
        }
    }
}

const PLANNER_PROMPT: &str = "\
You are a senior software architect. Your job is to brainstorm with the user \
to produce a clear, actionable design document. Ask one question at a time. \
When the design is complete, output a structured spec card.";

const INIT_AGENT_PROMPT: &str = "\
You are a project scaffolding agent. Given a design spec, create the initial \
project structure: README, CHANGELOG, directory layout, and configuration files.";

const CODER_PROMPT: &str = "\
You are an expert software engineer. Implement the plan step by step. \
Write clean, well-tested code. Use proper error handling.";

const REVIEWER_PROMPT: &str = "\
You are a code reviewer. Evaluate the implementation for correctness, \
architecture adherence, code quality, and test coverage. \
Output a verdict: APPROVE or REJECT with specific feedback.";

const DEFAULT_PROMPT: &str = "\
You are an AI assistant working within a pipeline stage. \
Complete the task assigned to this stage.";
