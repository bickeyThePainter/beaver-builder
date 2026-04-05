// ── Stage enum (mirrors Rust serde snake_case) ──────────────────────
export type Stage =
  | 'created'
  | 'planner'
  | 'init_agent'
  | 'coder'
  | 'reviewer'
  | 'human_review'
  | 'deploy'
  | 'push'
  | 'completed'
  | 'failed';

// ── Ops (Client → Server) — 8 variants ──────────────────────────────
export type Op =
  | { type: 'UserMessage'; payload: { task_id: string; content: string } }
  | { type: 'StartPipeline'; payload: { task_id: string; workspace_id: string } }
  | { type: 'AdvanceStage'; payload: { pipeline_id: string } }
  | { type: 'RevertStage'; payload: { pipeline_id: string; reason: string } }
  | { type: 'ApproveHumanReview'; payload: { pipeline_id: string } }
  | { type: 'RejectHumanReview'; payload: { pipeline_id: string; reason: string } }
  | { type: 'Deploy'; payload: { pipeline_id: string; environment: string } }
  | { type: 'InterruptPipeline'; payload: { pipeline_id: string } };

// ── Events (Server → Client) — 10 variants ──────────────────────────
export type Event =
  | { type: 'PipelineCreated'; payload: { pipeline_id: string; task_id: string; stage: Stage } }
  | { type: 'StageTransition'; payload: { pipeline_id: string; from: Stage; to: Stage; timestamp: string } }
  | { type: 'AgentOutput'; payload: { pipeline_id: string; stage: Stage; delta: string; is_final: boolean } }
  | { type: 'ToolExecution'; payload: { pipeline_id: string; tool: string; params: unknown; result: unknown; duration_ms: number } }
  | { type: 'ApprovalRequired'; payload: { pipeline_id: string; task_id: string; summary: string } }
  | { type: 'ReviewSubmitted'; payload: { pipeline_id: string; verdict: string; iteration: number } }
  | { type: 'DeployStatus'; payload: { pipeline_id: string; status: string; url: string | null } }
  | { type: 'PushComplete'; payload: { pipeline_id: string; remote: string; sha: string } }
  | { type: 'Error'; payload: { pipeline_id: string | null; code: string; message: string } }
  | { type: 'Warning'; payload: { pipeline_id: string; message: string } };

// ── WebSocket envelope ───────────────────────────────────────────────
export type WsMessage =
  | { kind: 'op'; payload: Op }
  | { kind: 'event'; payload: Event };

// ── View models ──────────────────────────────────────────────────────

export interface TaskView {
  id: string;
  title: string;
  spec: string;
  workspaceId: string;
  pipelineId: string | null;
  currentStage: Stage;
  status: string;
  logs: string[];
}

export interface WorkspaceView {
  id: string;
  name: string;
  repos: string[];
  worktrees: WorktreeView[];
}

export interface WorktreeView {
  id: string;
  branch: string;
  status: string;
  files: FileEntry[];
}

export interface FileEntry {
  name: string;
  type: string;
  size: string;
  author: string;
}

export interface ChatMessage {
  role: 'user' | 'agent';
  content: string;
}

export interface SpecCard {
  title: string;
  description: string;
  workspace: string;
  techStack: string[];
}

export type ViewName = 'dashboard' | 'workspaces' | 'planner-chat' | 'review';

// ── Pipeline stages for display ──────────────────────────────────────
export const PIPELINE_STAGES: Stage[] = [
  'planner',
  'init_agent',
  'coder',
  'reviewer',
  'human_review',
  'deploy',
  'push',
];

export const STAGE_LABELS: Record<Stage, string> = {
  created: 'Created',
  planner: 'Plan',
  init_agent: 'Init',
  coder: 'Code',
  reviewer: 'Review',
  human_review: 'Approve',
  deploy: 'Deploy',
  push: 'Push',
  completed: 'Done',
  failed: 'Failed',
};
