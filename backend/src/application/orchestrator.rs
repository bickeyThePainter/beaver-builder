use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::domain::agent::AgentConfig;
use crate::domain::pipeline::{Pipeline, Stage};
use crate::domain::task::Task;
use crate::infrastructure::llm_client::{LlmClient, LlmMessage, LlmRequest};
use crate::protocol::events::Event;
use crate::protocol::ops::Op;

/// The single-writer orchestrator. All domain state mutations flow through here.
///
/// Architecture: reads Ops from the Submission Queue, validates transitions,
/// mutates state, and publishes Events to the Event Queue.
pub struct PipelineOrchestrator {
    sq_rx: mpsc::Receiver<Op>,
    eq_tx: broadcast::Sender<Event>,
    llm: Arc<LlmClient>,
    pipelines: HashMap<String, Pipeline>,
    tasks: HashMap<String, Task>,
    /// Per-pipeline conversation history for the active stage agent.
    conversations: HashMap<String, Vec<LlmMessage>>,
    next_id: u64,
}

impl PipelineOrchestrator {
    pub fn new(sq_rx: mpsc::Receiver<Op>, eq_tx: broadcast::Sender<Event>) -> Self {
        Self {
            sq_rx,
            eq_tx,
            llm: Arc::new(LlmClient::from_env()),
            pipelines: HashMap::new(),
            tasks: HashMap::new(),
            conversations: HashMap::new(),
            next_id: 1,
        }
    }

    /// Main event loop. Runs until the submission queue is closed.
    pub async fn run(mut self) {
        tracing::info!("Orchestrator started");

        while let Some(op) = self.sq_rx.recv().await {
            if let Err(e) = self.handle_op(op).await {
                tracing::error!("Op handling failed: {e}");
                let _ = self.eq_tx.send(Event::Error {
                    pipeline_id: None,
                    code: "op_failed".into(),
                    message: e.to_string(),
                });
            }
        }

        tracing::info!("Orchestrator shutting down");
    }

    async fn handle_op(&mut self, op: Op) -> Result<(), Box<dyn std::error::Error>> {
        match op {
            Op::StartPipeline { task_id, workspace_id } => {
                let pipeline_id = self.generate_id("pl");
                let mut pipeline = Pipeline::new(pipeline_id.clone(), task_id.clone());

                // Create task if it doesn't exist yet
                if !self.tasks.contains_key(&task_id) {
                    let task = Task::new(task_id.clone(), "Untitled".into(), workspace_id);
                    self.tasks.insert(task_id.clone(), task);
                }

                if let Some(task) = self.tasks.get_mut(&task_id) {
                    task.attach_pipeline(pipeline_id.clone());
                }

                // Advance from Created to IntentClarifier
                let transition = pipeline.advance()?;
                self.emit(Event::PipelineCreated {
                    pipeline_id: pipeline_id.clone(),
                    task_id,
                    stage: transition.to,
                });
                self.emit(Event::StageTransition {
                    pipeline_id: pipeline_id.clone(),
                    from: transition.from,
                    to: transition.to,
                    timestamp: transition.timestamp.to_rfc3339(),
                });

                self.pipelines.insert(pipeline_id, pipeline);
            }

            Op::AdvanceStage { pipeline_id } => {
                let pipeline = self.get_pipeline_mut(&pipeline_id)?;
                let transition = pipeline.advance()?;
                self.emit(Event::StageTransition {
                    pipeline_id,
                    from: transition.from,
                    to: transition.to,
                    timestamp: transition.timestamp.to_rfc3339(),
                });
            }

            Op::RevertStage { pipeline_id, reason } => {
                let pipeline = self.get_pipeline_mut(&pipeline_id)?;

                match pipeline.revert_to_coder(reason.clone()) {
                    Ok(transition) => {
                        self.emit(Event::StageTransition {
                            pipeline_id,
                            from: transition.from,
                            to: transition.to,
                            timestamp: transition.timestamp.to_rfc3339(),
                        });
                    }
                    Err(crate::domain::pipeline::TransitionError::ReviewLoopExhausted { iterations }) => {
                        // Auto-escalate to human review
                        let pipeline = self.get_pipeline_mut(&pipeline_id)?;
                        let transition = pipeline.force_human_review()?;
                        self.emit(Event::Warning {
                            pipeline_id: Some(pipeline_id.clone()),
                            message: format!("Review loop exhausted after {iterations} iterations. Escalating to human review."),
                        });
                        self.emit(Event::StageTransition {
                            pipeline_id: pipeline_id.clone(),
                            from: transition.from,
                            to: transition.to,
                            timestamp: transition.timestamp.to_rfc3339(),
                        });
                        self.emit(Event::ApprovalRequired {
                            pipeline_id: pipeline_id.clone(),
                            task_id: self.pipelines.get(&pipeline_id).map(|p| p.task_id.clone()).unwrap_or_default(),
                            summary: "Review loop exhausted. Manual review required.".into(),
                        });
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            Op::ApproveHumanReview { pipeline_id } => {
                let pipeline = self.get_pipeline_mut(&pipeline_id)?;
                let transition = pipeline.advance()?;
                self.emit(Event::StageTransition {
                    pipeline_id,
                    from: transition.from,
                    to: transition.to,
                    timestamp: transition.timestamp.to_rfc3339(),
                });
            }

            Op::RejectHumanReview { pipeline_id, reason } => {
                let pipeline = self.get_pipeline_mut(&pipeline_id)?;
                let transition = pipeline.revert_to_coder(reason)?;
                self.emit(Event::StageTransition {
                    pipeline_id,
                    from: transition.from,
                    to: transition.to,
                    timestamp: transition.timestamp.to_rfc3339(),
                });
            }

            Op::UserMessage { task_id, content } => {
                let pipeline_id = self.tasks.get(&task_id)
                    .and_then(|t| t.pipeline_id.clone())
                    .unwrap_or_default();
                if pipeline_id.is_empty() {
                    return Err(format!("Task {task_id} has no attached pipeline").into());
                }

                // Determine current stage and its agent config
                let current_stage = self.pipelines.get(&pipeline_id)
                    .map(|p| p.current_stage())
                    .unwrap_or(Stage::IntentClarifier);
                let agent_cfg = AgentConfig::for_stage(current_stage);

                // Build conversation history
                let conv = self.conversations.entry(pipeline_id.clone()).or_insert_with(|| {
                    vec![LlmMessage {
                        role: "system".into(),
                        content: agent_cfg.system_prompt.clone(),
                    }]
                });
                conv.push(LlmMessage {
                    role: "user".into(),
                    content: content.clone(),
                });

                // Call OpenAI
                let llm = self.llm.clone();
                let messages = conv.clone();
                let model = agent_cfg.model.clone();
                let temperature = agent_cfg.temperature;
                let max_tokens = agent_cfg.max_tokens;

                let result = llm.chat(LlmRequest {
                    model,
                    messages,
                    temperature,
                    max_tokens,
                    tools: None,
                }).await;

                match result {
                    Ok(response) => {
                        // Record assistant reply in conversation
                        if let Some(conv) = self.conversations.get_mut(&pipeline_id) {
                            conv.push(LlmMessage {
                                role: "assistant".into(),
                                content: response.content.clone(),
                            });
                        }
                        self.emit(Event::AgentOutput {
                            pipeline_id,
                            stage: current_stage,
                            delta: response.content,
                            is_final: true,
                        });
                    }
                    Err(e) => {
                        tracing::error!("LLM call failed: {e}");
                        self.emit(Event::Error {
                            pipeline_id: Some(pipeline_id),
                            code: "llm_error".into(),
                            message: e.to_string(),
                        });
                    }
                }
            }

            Op::Deploy { pipeline_id, environment } => {
                self.emit(Event::DeployStatus {
                    pipeline_id,
                    status: format!("deploying to {environment}"),
                    url: None,
                });
                // Actual deployment logic delegated to infrastructure
            }

            Op::Push { pipeline_id, remote, branch: _ } => {
                self.emit(Event::PushComplete {
                    pipeline_id,
                    remote,
                    sha: "placeholder".into(),
                });
            }

            Op::InterruptPipeline { pipeline_id } => {
                let pipeline = self.get_pipeline_mut(&pipeline_id)?;
                pipeline.fail("Interrupted by user".into())?;
                self.emit(Event::Warning {
                    pipeline_id: Some(pipeline_id),
                    message: "Pipeline interrupted by user".into(),
                });
            }
        }

        Ok(())
    }

    fn get_pipeline_mut(&mut self, id: &str) -> Result<&mut Pipeline, Box<dyn std::error::Error>> {
        self.pipelines.get_mut(id)
            .ok_or_else(|| format!("Pipeline {id} not found").into())
    }

    fn emit(&self, event: Event) {
        // Ignore send errors (no subscribers is fine)
        let _ = self.eq_tx.send(event);
    }

    fn generate_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}_{}", self.next_id);
        self.next_id += 1;
        id
    }
}
