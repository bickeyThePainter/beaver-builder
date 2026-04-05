import { describe, it, expect, beforeEach } from 'vitest';
import { useStore } from '../store';
import type { Event, TaskView } from '../types';

// Reset store state before each test
beforeEach(() => {
  useStore.setState({
    connected: false,
    sendOp: null,
    currentView: 'dashboard',
    tasks: [],
    selectedTaskId: null,
    workspaces: [
      { id: 'ws-default', name: 'Default Workspace', repos: [], worktrees: [] },
    ],
    selectedWorkspaceId: 'ws-default',
    activeWorktreeId: null,
    messages: [
      { role: 'agent', content: "Hello! I'm the Planner agent. Describe the project you want to build and I'll help you create a design document." },
    ],
    generatedSpec: null,
    activeTaskId: null,
    reviewPipelineId: null,
    reviewSummary: null,
  });
});

// Helper: seed a task so event handlers have something to mutate
function seedTask(overrides: Partial<TaskView> = {}): TaskView {
  const task: TaskView = {
    id: 'task-001',
    title: 'Test Task',
    spec: '',
    workspaceId: 'ws-default',
    pipelineId: 'pipe-001',
    currentStage: 'planner',
    status: 'Processing',
    logs: [],
    ...overrides,
  };
  useStore.getState().addTask(task);
  return task;
}

// ── handleEvent: All 10 event types ─────────────────────────────────

describe('handleEvent', () => {
  // F1: PipelineCreated
  it('PipelineCreated creates new task when task_id not found', () => {
    const event: Event = {
      type: 'PipelineCreated',
      payload: { pipeline_id: 'pipe-002', task_id: 'task-new', stage: 'planner' },
    };
    useStore.getState().handleEvent(event);

    const tasks = useStore.getState().tasks;
    expect(tasks.length).toBe(1);
    expect(tasks[0].id).toBe('task-new');
    expect(tasks[0].pipelineId).toBe('pipe-002');
    expect(tasks[0].currentStage).toBe('planner');
  });

  it('PipelineCreated updates existing task', () => {
    seedTask({ id: 'task-001', pipelineId: null, currentStage: 'created' });

    const event: Event = {
      type: 'PipelineCreated',
      payload: { pipeline_id: 'pipe-001', task_id: 'task-001', stage: 'planner' },
    };
    useStore.getState().handleEvent(event);

    const tasks = useStore.getState().tasks;
    expect(tasks.length).toBe(1);
    expect(tasks[0].pipelineId).toBe('pipe-001');
    expect(tasks[0].currentStage).toBe('planner');
  });

  // F2: StageTransition
  it('StageTransition updates task stage and status', () => {
    seedTask();

    const event: Event = {
      type: 'StageTransition',
      payload: {
        pipeline_id: 'pipe-001',
        from: 'planner',
        to: 'init_agent',
        timestamp: '2026-04-05T10:00:00Z',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.currentStage).toBe('init_agent');
    expect(task.status).toBe('Processing');
    expect(task.logs).toContain('[transition] planner \u2192 init_agent');
  });

  it('StageTransition to completed sets status Completed', () => {
    seedTask({ currentStage: 'push' });

    const event: Event = {
      type: 'StageTransition',
      payload: {
        pipeline_id: 'pipe-001',
        from: 'push',
        to: 'completed',
        timestamp: '2026-04-05T10:05:00Z',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.currentStage).toBe('completed');
    expect(task.status).toBe('Completed');
  });

  it('StageTransition to failed sets status Failed', () => {
    seedTask({ currentStage: 'coder' });

    const event: Event = {
      type: 'StageTransition',
      payload: {
        pipeline_id: 'pipe-001',
        from: 'coder',
        to: 'failed',
        timestamp: '2026-04-05T10:05:00Z',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.currentStage).toBe('failed');
    expect(task.status).toBe('Failed');
  });

  // F3: AgentOutput (streaming)
  it('AgentOutput streaming appends to task logs', () => {
    seedTask();

    const event: Event = {
      type: 'AgentOutput',
      payload: {
        pipeline_id: 'pipe-001',
        stage: 'planner',
        delta: 'Here is a chunk of output',
        is_final: false,
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs.length).toBeGreaterThan(0);
    expect(task.logs[task.logs.length - 1]).toContain('[planner]');
  });

  // F4: AgentOutput (final)
  it('AgentOutput final adds chat message when planner', () => {
    seedTask();

    const event: Event = {
      type: 'AgentOutput',
      payload: {
        pipeline_id: 'pipe-001',
        stage: 'planner',
        delta: 'Final plan output',
        is_final: true,
      },
    };
    useStore.getState().handleEvent(event);

    const messages = useStore.getState().messages;
    const agentMessages = messages.filter((m) => m.content === 'Final plan output');
    expect(agentMessages.length).toBe(1);
    expect(agentMessages[0].role).toBe('agent');
  });

  it('AgentOutput final for non-planner stage does not add chat message', () => {
    seedTask({ currentStage: 'coder' });
    const initialMsgCount = useStore.getState().messages.length;

    const event: Event = {
      type: 'AgentOutput',
      payload: {
        pipeline_id: 'pipe-001',
        stage: 'coder',
        delta: 'Coder output',
        is_final: true,
      },
    };
    useStore.getState().handleEvent(event);

    // No new chat message added (only logged to task)
    expect(useStore.getState().messages.length).toBe(initialMsgCount);
  });

  // F5: ToolExecution
  it('ToolExecution logs tool invocation', () => {
    seedTask();

    const event: Event = {
      type: 'ToolExecution',
      payload: {
        pipeline_id: 'pipe-001',
        tool: 'create_file',
        params: { path: 'src/main.rs' },
        result: { ok: true },
        duration_ms: 150,
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs[task.logs.length - 1]).toContain('[tool] create_file (150ms)');
  });

  // F6: ApprovalRequired
  it('ApprovalRequired sets review pipeline and updates task status', () => {
    seedTask();

    const event: Event = {
      type: 'ApprovalRequired',
      payload: {
        pipeline_id: 'pipe-001',
        task_id: 'task-001',
        summary: 'Ready for human review.',
      },
    };
    useStore.getState().handleEvent(event);

    const state = useStore.getState();
    expect(state.reviewPipelineId).toBe('pipe-001');
    expect(state.reviewSummary).toBe('Ready for human review.');

    const task = state.tasks[0];
    expect(task.status).toBe('Awaiting Approval');
    expect(task.logs[task.logs.length - 1]).toContain('Approval required');
  });

  // F7: ReviewSubmitted
  it('ReviewSubmitted logs verdict and iteration', () => {
    seedTask();

    const event: Event = {
      type: 'ReviewSubmitted',
      payload: {
        pipeline_id: 'pipe-001',
        verdict: 'rejected',
        iteration: 2,
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs[task.logs.length - 1]).toContain('[review] rejected (iteration 2)');
  });

  // F8: DeployStatus
  it('DeployStatus updates task status and logs URL', () => {
    seedTask({ currentStage: 'deploy' });

    const event: Event = {
      type: 'DeployStatus',
      payload: {
        pipeline_id: 'pipe-001',
        status: 'success',
        url: 'https://staging.example.com/api/v1',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.status).toBe('Deploy: success');
    expect(task.logs[task.logs.length - 1]).toContain('https://staging.example.com/api/v1');
  });

  it('DeployStatus with null url still logs', () => {
    seedTask({ currentStage: 'deploy' });

    const event: Event = {
      type: 'DeployStatus',
      payload: {
        pipeline_id: 'pipe-001',
        status: 'in_progress',
        url: null,
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.status).toBe('Deploy: in_progress');
    expect(task.logs[task.logs.length - 1]).toContain('[deploy] in_progress');
  });

  // F9: PushComplete
  it('PushComplete logs remote and sha', () => {
    seedTask({ currentStage: 'push' });

    const event: Event = {
      type: 'PushComplete',
      payload: {
        pipeline_id: 'pipe-001',
        remote: 'origin',
        sha: 'abc123def456',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs[task.logs.length - 1]).toContain('[push] origin abc123def456');
  });

  // F10: Error
  it('Error with pipeline_id logs to task', () => {
    seedTask();

    const event: Event = {
      type: 'Error',
      payload: {
        pipeline_id: 'pipe-001',
        code: 'invalid_op',
        message: 'Cannot advance from completed stage',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs[task.logs.length - 1]).toContain(
      '[error] invalid_op: Cannot advance from completed stage'
    );
  });

  it('Error with null pipeline_id does not crash', () => {
    const event: Event = {
      type: 'Error',
      payload: {
        pipeline_id: null,
        code: 'system_error',
        message: 'Internal server error',
      },
    };

    // Should not throw
    expect(() => useStore.getState().handleEvent(event)).not.toThrow();
  });

  // F11: Warning
  it('Warning logs message to task', () => {
    seedTask();

    const event: Event = {
      type: 'Warning',
      payload: {
        pipeline_id: 'pipe-001',
        message: 'Review loop exhausted after 3 iterations',
      },
    };
    useStore.getState().handleEvent(event);

    const task = useStore.getState().tasks[0];
    expect(task.logs[task.logs.length - 1]).toContain(
      '[warning] Review loop exhausted after 3 iterations'
    );
  });
});

// ── Store actions ───────────────────────────────────────────────────

describe('store actions', () => {
  // F12: addTask
  it('addTask adds a task to the front of the list', () => {
    const task1: TaskView = {
      id: 'task-1',
      title: 'First Task',
      spec: 'Build something',
      workspaceId: 'ws-default',
      pipelineId: null,
      currentStage: 'created',
      status: 'New',
      logs: [],
    };
    const task2: TaskView = {
      id: 'task-2',
      title: 'Second Task',
      spec: 'Build another thing',
      workspaceId: 'ws-default',
      pipelineId: null,
      currentStage: 'created',
      status: 'New',
      logs: [],
    };

    useStore.getState().addTask(task1);
    useStore.getState().addTask(task2);

    const tasks = useStore.getState().tasks;
    expect(tasks.length).toBe(2);
    // Most recent task should be first (prepended)
    expect(tasks[0].id).toBe('task-2');
    expect(tasks[1].id).toBe('task-1');
  });

  // F13: selectTask
  it('selectTask updates selectedTaskId', () => {
    expect(useStore.getState().selectedTaskId).toBeNull();

    useStore.getState().selectTask('task-1');
    expect(useStore.getState().selectedTaskId).toBe('task-1');

    useStore.getState().selectTask(null);
    expect(useStore.getState().selectedTaskId).toBeNull();
  });

  // F14: resetChat
  it('resetChat clears messages and spec', () => {
    useStore.setState({
      messages: [
        { role: 'user', content: 'Hello' },
        { role: 'agent', content: 'Hi there' },
      ],
      generatedSpec: { title: 'Test', description: 'Test spec', workspace: 'ws-1', techStack: ['rust'] },
      activeTaskId: 'task-1',
    });

    useStore.getState().resetChat();

    const state = useStore.getState();
    // Should have the initial greeting message
    expect(state.messages.length).toBe(1);
    expect(state.messages[0].role).toBe('agent');
    expect(state.messages[0].content).toContain('Planner agent');
    expect(state.generatedSpec).toBeNull();
    expect(state.activeTaskId).toBeNull();
  });

  // F15: setConnected
  it('setConnected updates connected state', () => {
    expect(useStore.getState().connected).toBe(false);

    useStore.getState().setConnected(true);
    expect(useStore.getState().connected).toBe(true);

    useStore.getState().setConnected(false);
    expect(useStore.getState().connected).toBe(false);
  });

  // F16: setSendOp
  it('setSendOp registers a send function', () => {
    expect(useStore.getState().sendOp).toBeNull();

    let called = false;
    const mockSend = () => { called = true; };
    useStore.getState().setSendOp(mockSend);

    expect(useStore.getState().sendOp).toBe(mockSend);

    // Call it to verify it works
    useStore.getState().sendOp!({ type: 'AdvanceStage', payload: { pipeline_id: 'p1' } });
    expect(called).toBe(true);
  });

  it('setSendOp can be cleared', () => {
    useStore.getState().setSendOp(() => {});
    expect(useStore.getState().sendOp).not.toBeNull();

    useStore.getState().setSendOp(null);
    expect(useStore.getState().sendOp).toBeNull();
  });

  // setView
  it('setView changes current view', () => {
    expect(useStore.getState().currentView).toBe('dashboard');

    useStore.getState().setView('planner-chat');
    expect(useStore.getState().currentView).toBe('planner-chat');

    useStore.getState().setView('review');
    expect(useStore.getState().currentView).toBe('review');
  });

  // addMessage
  it('addMessage appends message to chat', () => {
    const initialCount = useStore.getState().messages.length;

    useStore.getState().addMessage({ role: 'user', content: 'Build me an API' });
    expect(useStore.getState().messages.length).toBe(initialCount + 1);

    const last = useStore.getState().messages[useStore.getState().messages.length - 1];
    expect(last.role).toBe('user');
    expect(last.content).toBe('Build me an API');
  });
});
