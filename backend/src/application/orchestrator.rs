use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::domain::agent::AgentConfig;
use crate::domain::pipeline::{Pipeline, Stage, TransitionError};
use crate::domain::task::Task;
use crate::llm::provider::{LlmMessage, LlmProvider, LlmRequest, Role};
use crate::protocol::events::Event;
use crate::protocol::ops::Op;

/// Single-writer orchestrator that consumes Ops from the SQ and publishes Events to the EQ.
pub struct PipelineOrchestrator {
    sq_rx: mpsc::Receiver<Op>,
    eq_tx: broadcast::Sender<Event>,
    llm: Arc<dyn LlmProvider>,
    pipelines: HashMap<String, Pipeline>,
    tasks: HashMap<String, Task>,
    conversations: HashMap<String, Vec<LlmMessage>>,
    next_id: u64,
}

impl PipelineOrchestrator {
    pub fn new(
        sq_rx: mpsc::Receiver<Op>,
        eq_tx: broadcast::Sender<Event>,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            sq_rx,
            eq_tx,
            llm,
            pipelines: HashMap::new(),
            tasks: HashMap::new(),
            conversations: HashMap::new(),
            next_id: 1,
        }
    }

    fn next_id(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("p{id}")
    }

    fn emit(&self, event: Event) {
        if let Err(e) = self.eq_tx.send(event) {
            warn!("no receivers for event: {e}");
        }
    }

    fn emit_all(&self, events: Vec<Event>) {
        for event in events {
            self.emit(event);
        }
    }

    /// Main event loop — runs until the SQ sender is dropped.
    pub async fn run(mut self) {
        info!("orchestrator started");
        while let Some(op) = self.sq_rx.recv().await {
            self.handle_op(op).await;
        }
        info!("orchestrator stopped (SQ closed)");
    }

    async fn handle_op(&mut self, op: Op) {
        match op {
            Op::StartPipeline {
                task_id,
                workspace_id,
            } => {
                self.handle_start_pipeline(task_id, workspace_id).await;
            }
            Op::UserMessage { task_id, content } => {
                self.handle_user_message(task_id, content).await;
            }
            Op::AdvanceStage { pipeline_id } => {
                self.handle_advance_stage(pipeline_id);
            }
            Op::RevertStage {
                pipeline_id,
                reason,
            } => {
                self.handle_revert_stage(pipeline_id, reason);
            }
            Op::ApproveHumanReview { pipeline_id } => {
                self.handle_approve_human_review(pipeline_id);
            }
            Op::RejectHumanReview {
                pipeline_id,
                reason,
            } => {
                self.handle_reject_human_review(pipeline_id, reason);
            }
            Op::Deploy {
                pipeline_id,
                environment,
            } => {
                self.handle_deploy(pipeline_id, environment);
            }
            Op::InterruptPipeline { pipeline_id } => {
                self.handle_interrupt(pipeline_id);
            }
        }
    }

    async fn handle_start_pipeline(&mut self, task_id: String, workspace_id: String) {
        let pipeline_id = self.next_id();

        // Create or update task
        let task = self
            .tasks
            .entry(task_id.clone())
            .or_insert_with(|| Task::new(task_id.clone(), task_id.clone(), String::new(), workspace_id));
        task.attach_pipeline(pipeline_id.clone());

        // Create pipeline and advance to Planner
        let mut pipeline = Pipeline::new(pipeline_id.clone(), task_id.clone());

        let mut events = vec![Event::PipelineCreated {
            pipeline_id: pipeline_id.clone(),
            task_id: task_id.clone(),
            stage: Stage::Created,
        }];

        match pipeline.advance() {
            Ok(transition) => {
                events.push(Event::StageTransition {
                    pipeline_id: pipeline_id.clone(),
                    from: transition.from,
                    to: transition.to,
                    timestamp: transition.timestamp,
                });
            }
            Err(e) => {
                error!("failed to advance new pipeline: {e}");
            }
        }

        self.pipelines.insert(pipeline_id, pipeline);
        self.emit_all(events);
    }

    async fn handle_user_message(&mut self, task_id: String, content: String) {
        // Find the pipeline for this task
        let pipeline_id = self
            .tasks
            .get(&task_id)
            .and_then(|t| t.pipeline_id.clone());

        let pipeline_id = match pipeline_id {
            Some(id) => id,
            None => {
                self.emit(Event::Error {
                    pipeline_id: None,
                    code: "NO_PIPELINE".into(),
                    message: format!("no pipeline for task {task_id}"),
                });
                return;
            }
        };

        let stage = match self.pipelines.get(&pipeline_id) {
            Some(p) => p.current_stage(),
            None => {
                self.emit(Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "PIPELINE_NOT_FOUND".into(),
                    message: "pipeline not found".into(),
                });
                return;
            }
        };

        // Build conversation
        let conversation = self
            .conversations
            .entry(pipeline_id.clone())
            .or_default();

        // Add system prompt if this is a fresh conversation
        if conversation.is_empty() {
            let config = AgentConfig::for_stage(stage);
            conversation.push(LlmMessage {
                role: Role::System,
                content: config.system_prompt.to_string(),
            });
        }

        // Add user message
        conversation.push(LlmMessage {
            role: Role::User,
            content: content.clone(),
        });

        // Get agent config for current stage
        let config = AgentConfig::for_stage(stage);

        let request = LlmRequest {
            model: config.model.to_string(),
            messages: conversation.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            stream: false,
        };

        // Call LLM
        match self.llm.chat(request).await {
            Ok(response) => {
                // Add assistant response to conversation
                let conversation = self
                    .conversations
                    .entry(pipeline_id.clone())
                    .or_default();
                conversation.push(LlmMessage {
                    role: Role::Assistant,
                    content: response.content.clone(),
                });

                self.emit(Event::AgentOutput {
                    pipeline_id,
                    stage,
                    delta: response.content,
                    is_final: true,
                });
            }
            Err(e) => {
                self.emit(Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "LLM_ERROR".into(),
                    message: e.to_string(),
                });
            }
        }
    }

    fn handle_advance_stage(&mut self, pipeline_id: String) {
        let events = {
            let pipeline = match self.pipelines.get_mut(&pipeline_id) {
                Some(p) => p,
                None => {
                    self.emit(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "PIPELINE_NOT_FOUND".into(),
                        message: "pipeline not found".into(),
                    });
                    return;
                }
            };

            match pipeline.advance() {
                Ok(transition) => {
                    let mut evts = Vec::new();
                    // If entering HumanReview, emit ApprovalRequired
                    if transition.to == Stage::HumanReview {
                        evts.push(Event::ApprovalRequired {
                            pipeline_id: pipeline_id.clone(),
                            task_id: pipeline.task_id.clone(),
                            summary: "Pipeline is ready for human review".into(),
                        });
                    }
                    evts.push(Event::StageTransition {
                        pipeline_id,
                        from: transition.from,
                        to: transition.to,
                        timestamp: transition.timestamp,
                    });
                    evts
                }
                Err(e) => {
                    vec![Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "TRANSITION_ERROR".into(),
                        message: e.to_string(),
                    }]
                }
            }
        };
        self.emit_all(events);
    }

    fn handle_revert_stage(&mut self, pipeline_id: String, reason: String) {
        let events = {
            let pipeline = match self.pipelines.get_mut(&pipeline_id) {
                Some(p) => p,
                None => {
                    self.emit(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "PIPELINE_NOT_FOUND".into(),
                        message: "pipeline not found".into(),
                    });
                    return;
                }
            };

            match pipeline.revert_to_coder(reason.clone()) {
                Ok(transition) => {
                    vec![
                        Event::ReviewSubmitted {
                            pipeline_id: pipeline_id.clone(),
                            verdict: "REJECT".into(),
                            iteration: pipeline.review_iterations,
                        },
                        Event::StageTransition {
                            pipeline_id,
                            from: transition.from,
                            to: transition.to,
                            timestamp: transition.timestamp,
                        },
                    ]
                }
                Err(TransitionError::ReviewLoopExhausted { iterations }) => {
                    debug!(
                        pipeline_id,
                        iterations, "review loop exhausted, auto-escalating"
                    );

                    let mut evts = vec![Event::Warning {
                        pipeline_id: pipeline_id.clone(),
                        message: format!(
                            "Review loop exhausted after {iterations} iterations. Auto-escalating to human review."
                        ),
                    }];

                    match pipeline.force_human_review() {
                        Ok(transition) => {
                            evts.push(Event::ApprovalRequired {
                                pipeline_id: pipeline_id.clone(),
                                task_id: pipeline.task_id.clone(),
                                summary: format!(
                                    "Auto-escalated after {iterations} failed review iterations"
                                ),
                            });
                            evts.push(Event::StageTransition {
                                pipeline_id,
                                from: transition.from,
                                to: transition.to,
                                timestamp: transition.timestamp,
                            });
                        }
                        Err(e) => {
                            evts.push(Event::Error {
                                pipeline_id: Some(pipeline_id),
                                code: "ESCALATION_FAILED".into(),
                                message: e.to_string(),
                            });
                        }
                    }
                    evts
                }
                Err(e) => {
                    vec![Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "REVERT_ERROR".into(),
                        message: e.to_string(),
                    }]
                }
            }
        };
        self.emit_all(events);
    }

    fn handle_approve_human_review(&mut self, pipeline_id: String) {
        let events = {
            let pipeline = match self.pipelines.get_mut(&pipeline_id) {
                Some(p) => p,
                None => {
                    self.emit(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "PIPELINE_NOT_FOUND".into(),
                        message: "pipeline not found".into(),
                    });
                    return;
                }
            };

            if pipeline.current_stage() != Stage::HumanReview {
                vec![Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "INVALID_STATE".into(),
                    message: "pipeline is not in human review stage".into(),
                }]
            } else {
                match pipeline.advance() {
                    Ok(transition) => {
                        vec![Event::StageTransition {
                            pipeline_id,
                            from: transition.from,
                            to: transition.to,
                            timestamp: transition.timestamp,
                        }]
                    }
                    Err(e) => {
                        vec![Event::Error {
                            pipeline_id: Some(pipeline_id),
                            code: "TRANSITION_ERROR".into(),
                            message: e.to_string(),
                        }]
                    }
                }
            }
        };
        self.emit_all(events);
    }

    fn handle_reject_human_review(&mut self, pipeline_id: String, reason: String) {
        let events = {
            let pipeline = match self.pipelines.get_mut(&pipeline_id) {
                Some(p) => p,
                None => {
                    self.emit(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "PIPELINE_NOT_FOUND".into(),
                        message: "pipeline not found".into(),
                    });
                    return;
                }
            };

            if pipeline.current_stage() != Stage::HumanReview {
                vec![Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "INVALID_STATE".into(),
                    message: "pipeline is not in human review stage".into(),
                }]
            } else {
                match pipeline.revert_to_coder(reason) {
                    Ok(transition) => {
                        vec![Event::StageTransition {
                            pipeline_id,
                            from: transition.from,
                            to: transition.to,
                            timestamp: transition.timestamp,
                        }]
                    }
                    Err(e) => {
                        vec![Event::Error {
                            pipeline_id: Some(pipeline_id),
                            code: "REVERT_ERROR".into(),
                            message: e.to_string(),
                        }]
                    }
                }
            }
        };
        self.emit_all(events);
    }

    fn handle_deploy(&mut self, pipeline_id: String, environment: String) {
        let current = self.pipelines.get(&pipeline_id).map(|p| p.current_stage());

        match current {
            None => {
                self.emit(Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "PIPELINE_NOT_FOUND".into(),
                    message: "pipeline not found".into(),
                });
            }
            Some(stage) if stage != Stage::Deploy => {
                self.emit(Event::Error {
                    pipeline_id: Some(pipeline_id),
                    code: "INVALID_STATE".into(),
                    message: "pipeline is not in deploy stage".into(),
                });
            }
            Some(_) => {
                self.emit(Event::DeployStatus {
                    pipeline_id,
                    status: format!("deploying to {environment}"),
                    url: None,
                });
            }
        }
    }

    fn handle_interrupt(&mut self, pipeline_id: String) {
        let events = {
            let pipeline = match self.pipelines.get_mut(&pipeline_id) {
                Some(p) => p,
                None => {
                    self.emit(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "PIPELINE_NOT_FOUND".into(),
                        message: "pipeline not found".into(),
                    });
                    return;
                }
            };

            let mut evts = vec![Event::Warning {
                pipeline_id: pipeline_id.clone(),
                message: "pipeline interrupted by user".into(),
            }];

            match pipeline.fail("interrupted by user".into()) {
                Ok(transition) => {
                    evts.push(Event::StageTransition {
                        pipeline_id,
                        from: transition.from,
                        to: transition.to,
                        timestamp: transition.timestamp,
                    });
                }
                Err(e) => {
                    evts.push(Event::Error {
                        pipeline_id: Some(pipeline_id),
                        code: "INTERRUPT_ERROR".into(),
                        message: e.to_string(),
                    });
                }
            }
            evts
        };
        self.emit_all(events);
    }
}
