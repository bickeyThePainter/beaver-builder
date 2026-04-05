import { create } from 'zustand';
import type {
  ChatMessage,
  Event,
  Op,
  SpecCard,
  Stage,
  TaskView,
  ViewName,
  WorkspaceView,
} from '../types';

export interface AppStore {
  // ── Connection ────────────────────────────────────────────────────
  connected: boolean;
  setConnected: (v: boolean) => void;

  // ── Send op (registered by useWebSocket) ──────────────────────────
  sendOp: ((op: Op) => void) | null;
  setSendOp: (fn: ((op: Op) => void) | null) => void;

  // ── Views ─────────────────────────────────────────────────────────
  currentView: ViewName;
  setView: (v: ViewName) => void;

  // ── Pipeline / Tasks ──────────────────────────────────────────────
  tasks: TaskView[];
  selectedTaskId: string | null;
  selectTask: (id: string | null) => void;
  addTask: (task: TaskView) => void;

  // ── Workspaces ────────────────────────────────────────────────────
  workspaces: WorkspaceView[];
  selectedWorkspaceId: string | null;
  activeWorktreeId: string | null;
  selectWorkspace: (id: string) => void;
  selectWorktree: (id: string) => void;

  // ── Chat ──────────────────────────────────────────────────────────
  messages: ChatMessage[];
  generatedSpec: SpecCard | null;
  activeTaskId: string | null;
  addMessage: (msg: ChatMessage) => void;
  setSpec: (spec: SpecCard | null) => void;
  resetChat: () => void;
  setActiveTaskId: (id: string | null) => void;

  // ── Review ────────────────────────────────────────────────────────
  reviewPipelineId: string | null;
  reviewSummary: string | null;
  setReviewPipeline: (pipelineId: string | null, summary: string | null) => void;

  // ── Event handler ─────────────────────────────────────────────────
  handleEvent: (event: Event) => void;
}

export const useStore = create<AppStore>((set, get) => ({
  // Connection
  connected: false,
  setConnected: (v) => set({ connected: v }),

  // Send op
  sendOp: null,
  setSendOp: (fn) => set({ sendOp: fn }),

  // Views
  currentView: 'dashboard',
  setView: (v) => set({ currentView: v }),

  // Tasks
  tasks: [],
  selectedTaskId: null,
  selectTask: (id) => set({ selectedTaskId: id }),
  addTask: (task) => set((s) => ({ tasks: [task, ...s.tasks] })),

  // Workspaces
  workspaces: [
    {
      id: 'ws-default',
      name: 'Default Workspace',
      repos: [],
      worktrees: [],
    },
  ],
  selectedWorkspaceId: 'ws-default',
  activeWorktreeId: null,
  selectWorkspace: (id) => set({ selectedWorkspaceId: id }),
  selectWorktree: (id) => set({ activeWorktreeId: id }),

  // Chat
  messages: [
    { role: 'agent', content: 'Hello! I\'m the Planner agent. Describe the project you want to build and I\'ll help you create a design document.' },
  ],
  generatedSpec: null,
  activeTaskId: null,
  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  setSpec: (spec) => set({ generatedSpec: spec }),
  resetChat: () =>
    set({
      messages: [
        { role: 'agent', content: 'Hello! I\'m the Planner agent. Describe the project you want to build and I\'ll help you create a design document.' },
      ],
      generatedSpec: null,
      activeTaskId: null,
    }),
  setActiveTaskId: (id) => set({ activeTaskId: id }),

  // Review
  reviewPipelineId: null,
  reviewSummary: null,
  setReviewPipeline: (pipelineId, summary) =>
    set({ reviewPipelineId: pipelineId, reviewSummary: summary }),

  // ── Handle ALL 10 event types ─────────────────────────────────────
  handleEvent: (event: Event) => {
    const state = get();

    switch (event.type) {
      case 'PipelineCreated': {
        const { pipeline_id, task_id, stage } = event.payload;
        // Check if task already exists, update it
        const existing = state.tasks.find((t) => t.id === task_id);
        if (existing) {
          set({
            tasks: state.tasks.map((t) =>
              t.id === task_id
                ? { ...t, pipelineId: pipeline_id, currentStage: stage }
                : t
            ),
          });
        } else {
          const newTask: TaskView = {
            id: task_id,
            title: task_id,
            spec: '',
            workspaceId: 'ws-default',
            pipelineId: pipeline_id,
            currentStage: stage,
            status: 'Initializing',
            logs: [`[system] Pipeline ${pipeline_id} created`],
          };
          set({ tasks: [newTask, ...state.tasks] });
        }
        break;
      }

      case 'StageTransition': {
        const { pipeline_id, to, from } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? {
                  ...t,
                  currentStage: to,
                  status: to === 'completed' ? 'Completed' : to === 'failed' ? 'Failed' : 'Processing',
                  logs: [...t.logs, `[transition] ${from} → ${to}`],
                }
              : t
          ),
        });
        break;
      }

      case 'AgentOutput': {
        const { pipeline_id, stage, delta, is_final } = event.payload;
        // Add to task logs
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[${stage}] ${delta.substring(0, 100)}`] }
              : t
          ),
        });
        // Also add to chat if in planner stage
        if (stage === 'planner' && is_final) {
          set((s) => ({
            messages: [...s.messages, { role: 'agent' as const, content: delta }],
          }));
        }
        break;
      }

      case 'ToolExecution': {
        const { pipeline_id, tool, duration_ms } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[tool] ${tool} (${duration_ms}ms)`] }
              : t
          ),
        });
        break;
      }

      case 'ApprovalRequired': {
        const { pipeline_id, summary } = event.payload;
        set({
          reviewPipelineId: pipeline_id,
          reviewSummary: summary,
        });
        // Add log
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, status: 'Awaiting Approval', logs: [...t.logs, `[system] Approval required: ${summary}`] }
              : t
          ),
        });
        break;
      }

      case 'ReviewSubmitted': {
        const { pipeline_id, verdict, iteration } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[review] ${verdict} (iteration ${iteration})`] }
              : t
          ),
        });
        break;
      }

      case 'DeployStatus': {
        const { pipeline_id, status, url } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? {
                  ...t,
                  status: `Deploy: ${status}`,
                  logs: [...t.logs, `[deploy] ${status}${url ? ` → ${url}` : ''}`],
                }
              : t
          ),
        });
        break;
      }

      case 'PushComplete': {
        const { pipeline_id, remote, sha } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[push] ${remote} ${sha}`] }
              : t
          ),
        });
        break;
      }

      case 'Error': {
        const { pipeline_id, code, message } = event.payload;
        if (pipeline_id) {
          set({
            tasks: state.tasks.map((t) =>
              t.pipelineId === pipeline_id
                ? { ...t, logs: [...t.logs, `[error] ${code}: ${message}`] }
                : t
            ),
          });
        }
        console.error(`[BB Error] ${code}: ${message}`);
        break;
      }

      case 'Warning': {
        const { pipeline_id, message } = event.payload;
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[warning] ${message}`] }
              : t
          ),
        });
        console.warn(`[BB Warning] ${message}`);
        break;
      }
    }
  },
}));
