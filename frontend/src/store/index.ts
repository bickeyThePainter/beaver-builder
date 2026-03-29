import { create } from 'zustand';
import type { Task, Workspace, ChatMessage, Spec, Event, Op } from '../types';

interface BeaverStore {
  // Connection
  connected: boolean;
  setConnected: (v: boolean) => void;

  // WebSocket sendOp -- set by useWebSocket hook
  sendOp: ((op: Op) => void) | null;
  setSendOp: (fn: (op: Op) => void) => void;

  // View
  currentView: 'dashboard' | 'workspaces' | 'intent-chat';
  setCurrentView: (v: BeaverStore['currentView']) => void;

  // Pipeline
  tasks: Task[];
  selectedTaskId: string | null;
  selectTask: (id: string | null) => void;

  // Workspace
  workspaces: Workspace[];
  selectedWorkspaceId: string | null;
  activeWorktreeId: string | null;
  selectWorkspace: (id: string) => void;
  selectWorktree: (id: string) => void;

  // Chat -- tracks the active task being discussed with the Intent Clarifier
  activeTaskId: string | null;
  messages: ChatMessage[];
  generatedSpec: Spec | null;
  addMessage: (msg: ChatMessage) => void;
  setGeneratedSpec: (spec: Spec | null) => void;
  resetChat: () => void;

  // Event dispatcher
  handleEvent: (event: Event) => void;

  // Task management
  addTask: (task: Task) => void;
}

const INITIAL_CHAT: ChatMessage[] = [
  { role: 'agent', content: "Hello. I'm the Intent Clarifier. Describe the coding task you'd like to build." },
];

export const useBeaverStore = create<BeaverStore>((set, get) => ({
  connected: false,
  setConnected: (v) => set({ connected: v }),

  sendOp: null,
  setSendOp: (fn) => set({ sendOp: fn }),

  currentView: 'dashboard',
  setCurrentView: (v) => set({ currentView: v }),

  tasks: [],
  selectedTaskId: null,
  selectTask: (id) => set({ selectedTaskId: id }),

  workspaces: [],
  selectedWorkspaceId: null,
  activeWorktreeId: null,
  selectWorkspace: (id) => {
    const ws = get().workspaces.find((w) => w.id === id);
    set({
      selectedWorkspaceId: id,
      activeWorktreeId: ws?.worktrees[0]?.id ?? null,
    });
  },
  selectWorktree: (id) => set({ activeWorktreeId: id }),

  activeTaskId: null,
  messages: [...INITIAL_CHAT],
  generatedSpec: null,
  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  setGeneratedSpec: (spec) => set({ generatedSpec: spec }),
  resetChat: () => set({ messages: [...INITIAL_CHAT], generatedSpec: null, activeTaskId: null }),

  addTask: (task) => set((s) => ({ tasks: [task, ...s.tasks] })),

  handleEvent: (event) => {
    const state = get();

    switch (event.type) {
      case 'PipelineCreated': {
        const { pipeline_id, task_id, stage } = event.payload;
        // If this task already exists (created optimistically), update it.
        // Otherwise create it from the event.
        const existing = state.tasks.find((t) => t.id === task_id);
        if (existing) {
          set({
            tasks: state.tasks.map((t) =>
              t.id === task_id
                ? { ...t, pipelineId: pipeline_id, currentStage: stage, status: 'processing' }
                : t
            ),
          });
        } else {
          set({
            tasks: [
              {
                id: task_id,
                title: 'Pipeline Task',
                spec: '',
                workspaceId: '',
                pipelineId: pipeline_id,
                currentStage: stage,
                status: 'processing',
                logs: [`[system] Pipeline created at stage ${stage}`],
                priority: 'medium',
              },
              ...state.tasks,
            ],
          });
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
                  status: to === 'completed' ? 'completed' : to === 'failed' ? 'failed' : to === 'human_review' ? 'awaiting_approval' : 'processing',
                  logs: [...t.logs, `[transition] ${from} -> ${to}`],
                }
              : t
          ),
        });
        break;
      }

      case 'AgentOutput': {
        const { pipeline_id, delta, stage } = event.payload;
        // Also append agent output to chat messages if this is for the active task
        const activeTaskId = state.activeTaskId;
        const activeTask = activeTaskId ? state.tasks.find((t) => t.id === activeTaskId) : null;
        if (activeTask && activeTask.pipelineId === pipeline_id && stage === 'intent_clarifier') {
          set((s) => ({ messages: [...s.messages, { role: 'agent' as const, content: delta }] }));
        }
        set({
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, logs: [...t.logs, `[agent:${stage}] ${delta}`] }
              : t
          ),
        });
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
          tasks: state.tasks.map((t) =>
            t.pipelineId === pipeline_id
              ? { ...t, status: 'awaiting_approval', logs: [...t.logs, `[approval] ${summary}`] }
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
              ? { ...t, logs: [...t.logs, `[review] verdict=${verdict} iteration=${iteration}`] }
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
              ? { ...t, logs: [...t.logs, `[deploy] ${status}${url ? ` (${url})` : ''}`] }
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
              ? { ...t, logs: [...t.logs, `[push] ${remote} @ ${sha}`] }
              : t
          ),
        });
        break;
      }

      case 'Warning': {
        const { pipeline_id, message } = event.payload;
        if (pipeline_id) {
          set({
            tasks: state.tasks.map((t) =>
              t.pipelineId === pipeline_id
                ? { ...t, logs: [...t.logs, `[warning] ${message}`] }
                : t
            ),
          });
        }
        break;
      }

      case 'Error': {
        const { pipeline_id, message } = event.payload;
        if (pipeline_id) {
          set({
            tasks: state.tasks.map((t) =>
              t.pipelineId === pipeline_id
                ? { ...t, status: 'failed', logs: [...t.logs, `[error] ${message}`] }
                : t
            ),
          });
        }
        break;
      }

      default:
        break;
    }
  },
}));
