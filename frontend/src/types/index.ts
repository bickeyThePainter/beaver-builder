// ---------------------------------------------------------------------------
// Protocol types -- mirrors backend/src/protocol/
// ---------------------------------------------------------------------------

// --- Ops (Client -> Server) ---

export type Op =
  | { type: 'UserMessage'; payload: { task_id: string; content: string } }
  | { type: 'StartPipeline'; payload: { task_id: string; workspace_id: string } }
  | { type: 'AdvanceStage'; payload: { pipeline_id: string } }
  | { type: 'RevertStage'; payload: { pipeline_id: string; reason: string } }
  | { type: 'ApproveHumanReview'; payload: { pipeline_id: string } }
  | { type: 'RejectHumanReview'; payload: { pipeline_id: string; reason: string } }
  | { type: 'Deploy'; payload: { pipeline_id: string; environment: string } }
  | { type: 'Push'; payload: { pipeline_id: string; remote: string; branch: string } }
  | { type: 'InterruptPipeline'; payload: { pipeline_id: string } };

// --- Events (Server -> Client) ---

export type Event =
  | { type: 'PipelineCreated'; payload: { pipeline_id: string; task_id: string; stage: string } }
  | { type: 'StageTransition'; payload: { pipeline_id: string; from: string; to: string; timestamp: string } }
  | { type: 'AgentOutput'; payload: { pipeline_id: string; stage: string; delta: string; is_final: boolean } }
  | { type: 'ToolExecution'; payload: { pipeline_id: string; tool: string; params: unknown; result: unknown; duration_ms: number } }
  | { type: 'ApprovalRequired'; payload: { pipeline_id: string; task_id: string; summary: string } }
  | { type: 'ReviewSubmitted'; payload: { pipeline_id: string; verdict: string; iteration: number } }
  | { type: 'DeployStatus'; payload: { pipeline_id: string; status: string; url: string | null } }
  | { type: 'PushComplete'; payload: { pipeline_id: string; remote: string; sha: string } }
  | { type: 'Error'; payload: { pipeline_id: string | null; code: string; message: string } }
  | { type: 'Warning'; payload: { pipeline_id: string | null; message: string } };

// --- WebSocket frame ---

export type WsMessage =
  | { kind: 'op'; payload: Op }
  | { kind: 'event'; payload: Event };

// ---------------------------------------------------------------------------
// Domain types -- frontend view models
// ---------------------------------------------------------------------------

export const PIPELINE_STAGES = [
  'intent_clarifier',
  'init_agent',
  'planner',
  'coder',
  'reviewer',
  'human_review',
  'deploy',
  'push',
] as const;

export type StageId = typeof PIPELINE_STAGES[number];

export interface StageInfo {
  id: StageId;
  label: string;
}

export const STAGE_INFO: StageInfo[] = [
  { id: 'intent_clarifier', label: 'Intent' },
  { id: 'init_agent', label: 'Init' },
  { id: 'planner', label: 'Planner' },
  { id: 'coder', label: 'Coder' },
  { id: 'reviewer', label: 'Reviewer' },
  { id: 'human_review', label: 'Approve' },
  { id: 'deploy', label: 'Deploy' },
  { id: 'push', label: 'Push' },
];

export type Priority = 'low' | 'medium' | 'high' | 'critical';
export type TaskStatus = 'initializing' | 'processing' | 'awaiting_approval' | 'completed' | 'failed';

export interface Task {
  id: string;
  title: string;
  spec: string;
  workspaceId: string;
  pipelineId: string | null;
  currentStage: string;
  status: TaskStatus;
  logs: string[];
  priority: Priority;
}

export type WorktreeStatus = 'active' | 'processing' | 'idle';

export interface FileArtifact {
  name: string;
  type: string;
  size: string;
  author: string;
}

export interface Worktree {
  id: string;
  branch: string;
  status: WorktreeStatus;
  files: FileArtifact[];
}

export interface Workspace {
  id: string;
  name: string;
  repos: string[];
  swimlane: { active: string; base: string };
  worktrees: Worktree[];
}

export interface ChatMessage {
  role: 'user' | 'agent';
  content: string;
}

export interface Spec {
  title: string;
  description: string;
  workspace: string;
  techStack: string[];
}
