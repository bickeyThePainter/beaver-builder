/**
 * Tests for the Zustand store (useBeaverStore).
 *
 * Covers:
 *   - handleEvent for every event type
 *   - Task creation from PipelineCreated
 *   - Stage transitions updating task state
 *   - AgentOutput appending to logs
 *   - Error setting task status to failed
 *   - Store actions: addMessage, resetChat, selectWorkspace, selectTask
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useBeaverStore } from '../store';
import type { Event, Task, Workspace } from '../types';

// Reset store before each test
beforeEach(() => {
  const state = useBeaverStore.getState();
  useBeaverStore.setState({
    tasks: [],
    messages: [{ role: 'agent', content: "Hello. I'm the Intent Clarifier. Describe the coding task you'd like to build." }],
    generatedSpec: null,
    activeTaskId: null,
    selectedTaskId: null,
    selectedWorkspaceId: null,
    activeWorktreeId: null,
    connected: false,
    workspaces: [],
  });
});

// ---------------------------------------------------------------------------
// Helper: create a task in the store
// ---------------------------------------------------------------------------
function seedTask(overrides: Partial<Task> = {}): Task {
  const task: Task = {
    id: 'task_01',
    title: 'Test Task',
    spec: 'Build something',
    workspaceId: 'ws_01',
    pipelineId: 'pl_01',
    currentStage: 'coder',
    status: 'processing',
    logs: [],
    priority: 'medium',
    ...overrides,
  };
  useBeaverStore.getState().addTask(task);
  return task;
}

// ---------------------------------------------------------------------------
// PipelineCreated
// ---------------------------------------------------------------------------
describe('handleEvent: PipelineCreated', () => {
  it('creates a new task if one does not exist', () => {
    const event: Event = {
      type: 'PipelineCreated',
      payload: { pipeline_id: 'pl_new', task_id: 'task_new', stage: 'intent_clarifier' },
    };

    useBeaverStore.getState().handleEvent(event);
    const tasks = useBeaverStore.getState().tasks;

    expect(tasks.length).toBe(1);
    expect(tasks[0].id).toBe('task_new');
    expect(tasks[0].pipelineId).toBe('pl_new');
    expect(tasks[0].currentStage).toBe('intent_clarifier');
    expect(tasks[0].status).toBe('processing');
  });

  it('updates an existing task with pipeline info', () => {
    seedTask({ id: 'task_01', pipelineId: null, currentStage: '', status: 'initializing' });

    const event: Event = {
      type: 'PipelineCreated',
      payload: { pipeline_id: 'pl_01', task_id: 'task_01', stage: 'intent_clarifier' },
    };

    useBeaverStore.getState().handleEvent(event);
    const tasks = useBeaverStore.getState().tasks;

    expect(tasks.length).toBe(1);
    expect(tasks[0].pipelineId).toBe('pl_01');
    expect(tasks[0].currentStage).toBe('intent_clarifier');
    expect(tasks[0].status).toBe('processing');
  });
});

// ---------------------------------------------------------------------------
// StageTransition
// ---------------------------------------------------------------------------
describe('handleEvent: StageTransition', () => {
  it('updates task currentStage and appends transition log', () => {
    seedTask();

    const event: Event = {
      type: 'StageTransition',
      payload: { pipeline_id: 'pl_01', from: 'coder', to: 'reviewer', timestamp: '2026-03-29T10:00:00Z' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.currentStage).toBe('reviewer');
    expect(task.status).toBe('processing');
    expect(task.logs).toContain('[transition] coder -> reviewer');
  });

  it('sets status to completed when transitioning to completed', () => {
    seedTask({ currentStage: 'push' });

    const event: Event = {
      type: 'StageTransition',
      payload: { pipeline_id: 'pl_01', from: 'push', to: 'completed', timestamp: '2026-03-29T10:00:00Z' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.currentStage).toBe('completed');
    expect(task.status).toBe('completed');
  });

  it('sets status to failed when transitioning to failed', () => {
    seedTask();

    const event: Event = {
      type: 'StageTransition',
      payload: { pipeline_id: 'pl_01', from: 'coder', to: 'failed', timestamp: '2026-03-29T10:00:00Z' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.status).toBe('failed');
  });

  it('sets status to awaiting_approval when transitioning to human_review', () => {
    seedTask({ currentStage: 'reviewer' });

    const event: Event = {
      type: 'StageTransition',
      payload: { pipeline_id: 'pl_01', from: 'reviewer', to: 'human_review', timestamp: '2026-03-29T10:00:00Z' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.status).toBe('awaiting_approval');
  });
});

// ---------------------------------------------------------------------------
// AgentOutput
// ---------------------------------------------------------------------------
describe('handleEvent: AgentOutput', () => {
  it('appends delta to task logs', () => {
    seedTask();

    const event: Event = {
      type: 'AgentOutput',
      payload: { pipeline_id: 'pl_01', stage: 'coder', delta: 'Writing main.rs', is_final: false },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('Writing main.rs'))).toBe(true);
  });

  it('appends agent output to chat messages for active task intent_clarifier', () => {
    const task = seedTask({ id: 'task_active', pipelineId: 'pl_active' });
    useBeaverStore.setState({ activeTaskId: 'task_active' });

    const event: Event = {
      type: 'AgentOutput',
      payload: { pipeline_id: 'pl_active', stage: 'intent_clarifier', delta: 'Let me clarify', is_final: false },
    };

    useBeaverStore.getState().handleEvent(event);
    const messages = useBeaverStore.getState().messages;

    expect(messages.some(m => m.content === 'Let me clarify' && m.role === 'agent')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// ToolExecution
// ---------------------------------------------------------------------------
describe('handleEvent: ToolExecution', () => {
  it('appends tool execution to task logs', () => {
    seedTask();

    const event: Event = {
      type: 'ToolExecution',
      payload: {
        pipeline_id: 'pl_01',
        tool: 'write_file',
        params: { path: 'src/main.rs' },
        result: { success: true },
        duration_ms: 42,
      },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('write_file') && l.includes('42ms'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// ApprovalRequired
// ---------------------------------------------------------------------------
describe('handleEvent: ApprovalRequired', () => {
  it('sets task status to awaiting_approval', () => {
    seedTask();

    const event: Event = {
      type: 'ApprovalRequired',
      payload: { pipeline_id: 'pl_01', task_id: 'task_01', summary: 'Ready for review' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.status).toBe('awaiting_approval');
    expect(task.logs.some(l => l.includes('Ready for review'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// ReviewSubmitted
// ---------------------------------------------------------------------------
describe('handleEvent: ReviewSubmitted', () => {
  it('appends review info to task logs', () => {
    seedTask();

    const event: Event = {
      type: 'ReviewSubmitted',
      payload: { pipeline_id: 'pl_01', verdict: 'request_changes', iteration: 2 },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('request_changes') && l.includes('2'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// DeployStatus
// ---------------------------------------------------------------------------
describe('handleEvent: DeployStatus', () => {
  it('appends deploy status to logs with url', () => {
    seedTask();

    const event: Event = {
      type: 'DeployStatus',
      payload: { pipeline_id: 'pl_01', status: 'success', url: 'https://staging.example.com' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('success') && l.includes('staging.example.com'))).toBe(true);
  });

  it('appends deploy status to logs without url', () => {
    seedTask();

    const event: Event = {
      type: 'DeployStatus',
      payload: { pipeline_id: 'pl_01', status: 'in_progress', url: null },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('in_progress'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// PushComplete
// ---------------------------------------------------------------------------
describe('handleEvent: PushComplete', () => {
  it('appends push info to logs', () => {
    seedTask();

    const event: Event = {
      type: 'PushComplete',
      payload: { pipeline_id: 'pl_01', remote: 'origin', sha: 'abc123' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('origin') && l.includes('abc123'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------
describe('handleEvent: Error', () => {
  it('sets task status to failed when pipeline_id is present', () => {
    seedTask();

    const event: Event = {
      type: 'Error',
      payload: { pipeline_id: 'pl_01', code: 'INVALID_TRANSITION', message: 'Cannot advance' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.status).toBe('failed');
    expect(task.logs.some(l => l.includes('Cannot advance'))).toBe(true);
  });

  it('does not crash when pipeline_id is null', () => {
    seedTask();

    const event: Event = {
      type: 'Error',
      payload: { pipeline_id: null, code: 'PARSE_ERROR', message: 'Bad JSON' },
    };

    // Should not throw
    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    // Task should be unchanged
    expect(task.status).toBe('processing');
  });
});

// ---------------------------------------------------------------------------
// Warning
// ---------------------------------------------------------------------------
describe('handleEvent: Warning', () => {
  it('appends warning to task logs when pipeline_id present', () => {
    seedTask();

    const event: Event = {
      type: 'Warning',
      payload: { pipeline_id: 'pl_01', message: 'Review loop exhausted' },
    };

    useBeaverStore.getState().handleEvent(event);
    const task = useBeaverStore.getState().tasks[0];

    expect(task.logs.some(l => l.includes('Review loop exhausted'))).toBe(true);
  });

  it('does not crash when pipeline_id is null', () => {
    const event: Event = {
      type: 'Warning',
      payload: { pipeline_id: null, message: 'System warning' },
    };

    // Should not throw
    useBeaverStore.getState().handleEvent(event);
  });
});

// ---------------------------------------------------------------------------
// Unknown event type
// ---------------------------------------------------------------------------
describe('handleEvent: unknown', () => {
  it('silently ignores unknown event types', () => {
    seedTask();

    const event = {
      type: 'SomeNewEventType',
      payload: { pipeline_id: 'pl_01', data: 'stuff' },
    } as unknown as Event;

    // Should not throw
    useBeaverStore.getState().handleEvent(event);

    // State unchanged
    const task = useBeaverStore.getState().tasks[0];
    expect(task.logs.length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Store actions
// ---------------------------------------------------------------------------
describe('Store actions', () => {
  it('addMessage appends to messages', () => {
    const initial = useBeaverStore.getState().messages.length;
    useBeaverStore.getState().addMessage({ role: 'user', content: 'Hello' });
    expect(useBeaverStore.getState().messages.length).toBe(initial + 1);
    expect(useBeaverStore.getState().messages[initial].content).toBe('Hello');
  });

  it('resetChat restores initial greeting', () => {
    useBeaverStore.getState().addMessage({ role: 'user', content: 'Test' });
    useBeaverStore.getState().addMessage({ role: 'agent', content: 'Reply' });
    useBeaverStore.getState().resetChat();

    const messages = useBeaverStore.getState().messages;
    expect(messages.length).toBe(1);
    expect(messages[0].role).toBe('agent');
    expect(messages[0].content).toContain('Intent Clarifier');
  });

  it('selectWorkspace sets workspace and first worktree', () => {
    const workspace: Workspace = {
      id: 'ws_01',
      name: 'My Workspace',
      repos: [],
      swimlane: { active: '', base: '' },
      worktrees: [
        { id: 'wt_01', branch: 'main', status: 'active', files: [] },
        { id: 'wt_02', branch: 'dev', status: 'idle', files: [] },
      ],
    };
    useBeaverStore.setState({ workspaces: [workspace] });
    useBeaverStore.getState().selectWorkspace('ws_01');

    expect(useBeaverStore.getState().selectedWorkspaceId).toBe('ws_01');
    expect(useBeaverStore.getState().activeWorktreeId).toBe('wt_01');
  });

  it('selectWorkspace with no worktrees sets null activeWorktreeId', () => {
    const workspace: Workspace = {
      id: 'ws_empty',
      name: 'Empty Workspace',
      repos: [],
      swimlane: { active: '', base: '' },
      worktrees: [],
    };
    useBeaverStore.setState({ workspaces: [workspace] });
    useBeaverStore.getState().selectWorkspace('ws_empty');

    expect(useBeaverStore.getState().selectedWorkspaceId).toBe('ws_empty');
    expect(useBeaverStore.getState().activeWorktreeId).toBeNull();
  });

  it('selectTask updates selectedTaskId', () => {
    useBeaverStore.getState().selectTask('task_42');
    expect(useBeaverStore.getState().selectedTaskId).toBe('task_42');

    useBeaverStore.getState().selectTask(null);
    expect(useBeaverStore.getState().selectedTaskId).toBeNull();
  });
});
