import React from 'react';
import { ShieldCheck } from 'lucide-react';
import { useBeaverStore } from './store';
import { useWebSocket } from './hooks/useWebSocket';
import { Navbar } from './components/Layout/Navbar';
import { StatusBar } from './components/Layout/StatusBar';
import { PipelineCard } from './components/Pipeline/PipelineCard';
import { WorkspaceList } from './components/Workspace/WorkspaceList';
import { WorkspaceDetail } from './components/Workspace/WorkspaceDetail';
import { IntentChat } from './components/Chat/IntentChat';

const App: React.FC = () => {
  useWebSocket();

  const currentView = useBeaverStore((s) => s.currentView);
  const tasks = useBeaverStore((s) => s.tasks);
  const selectedTaskId = useBeaverStore((s) => s.selectedTaskId);
  const selectTask = useBeaverStore((s) => s.selectTask);
  const workspaces = useBeaverStore((s) => s.workspaces);
  const selectedWorkspaceId = useBeaverStore((s) => s.selectedWorkspaceId);
  const activeWorktreeId = useBeaverStore((s) => s.activeWorktreeId);
  const selectWorkspace = useBeaverStore((s) => s.selectWorkspace);
  const selectWorktree = useBeaverStore((s) => s.selectWorktree);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const selectedWorkspace = workspaces.find((w) => w.id === selectedWorkspaceId) ?? null;

  const taskCountsByWorkspace = tasks.reduce<Record<string, number>>((acc, t) => {
    acc[t.workspaceId] = (acc[t.workspaceId] ?? 0) + 1;
    return acc;
  }, {});

  const renderDashboard = () => (
    <div className="grid grid-cols-12 gap-6">
      {/* Pipeline cards */}
      <div className="col-span-8 space-y-4">
        <h2 className="text-lg font-semibold text-slate-400 mb-2 px-1">Pipeline Oversight</h2>
        {tasks.length === 0 && (
          <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-12 text-center">
            <p className="text-sm text-slate-600">No active pipelines. Click "New Task" to start one.</p>
          </div>
        )}
        {tasks.map((task) => (
          <PipelineCard
            key={task.id}
            task={task}
            isSelected={selectedTaskId === task.id}
            workspaceName={workspaces.find((w) => w.id === task.workspaceId)?.name ?? task.workspaceId}
            onClick={() => selectTask(task.id)}
          />
        ))}
      </div>

      {/* Detail panel */}
      <div className="col-span-4 space-y-6">
        <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 min-h-[400px]">
          {selectedTask ? (
            <div className="space-y-6">
              <div>
                <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3 flex items-center gap-2 underline decoration-indigo-500/50 underline-offset-4">
                  Active Specification
                </h4>
                <div className="text-xs text-slate-300 bg-black/40 p-4 rounded-xl border border-slate-800 leading-relaxed font-mono">
                  {selectedTask.spec || 'No specification yet.'}
                </div>
              </div>
              <div>
                <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3">Live Telemetry</h4>
                <div className="space-y-2">
                  {selectedTask.logs.map((log, i) => (
                    <div key={i} className="text-[10px] font-mono text-slate-500 flex gap-2">
                      <span className="text-indigo-600 shrink-0">&rsaquo;</span>
                      <span>{log}</span>
                    </div>
                  ))}
                  <div className="text-[10px] font-mono text-indigo-400 flex gap-2 animate-pulse">
                    <span className="shrink-0">&rsaquo;</span>
                    <span>[{selectedTask.currentStage}] exploring workspace...</span>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="h-full flex flex-col items-center justify-center text-slate-700 text-center min-h-[300px]">
              <ShieldCheck size={48} className="mb-4 opacity-10" />
              <p className="text-xs">Select a pipeline task to monitor agent activity.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );

  const renderWorkspaces = () => (
    <div className="grid grid-cols-12 gap-6">
      <div className="col-span-3">
        <WorkspaceList
          workspaces={workspaces}
          selectedId={selectedWorkspaceId}
          taskCounts={taskCountsByWorkspace}
          onSelect={selectWorkspace}
        />
      </div>
      <div className="col-span-9">
        {selectedWorkspace ? (
          <WorkspaceDetail
            workspace={selectedWorkspace}
            activeWorktreeId={activeWorktreeId}
            onSelectWorktree={selectWorktree}
          />
        ) : (
          <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-12 text-center min-h-[600px] flex items-center justify-center">
            <p className="text-sm text-slate-600">Select or provision a workspace to begin.</p>
          </div>
        )}
      </div>
    </div>
  );

  return (
    <div className="min-h-screen bg-[#07080a] text-slate-200 font-sans p-6 selection:bg-indigo-500/30">
      <Navbar />
      <main className="max-w-7xl mx-auto pb-20">
        {currentView === 'dashboard' && renderDashboard()}
        {currentView === 'workspaces' && renderWorkspaces()}
        {currentView === 'intent-chat' && <IntentChat />}
      </main>
      <StatusBar />
    </div>
  );
};

export default App;
