import { ShieldCheck } from 'lucide-react';
import { PipelineCard } from './components/Pipeline/PipelineCard';
import { PlannerChat } from './components/Chat/PlannerChat';
import { ReviewPanel } from './components/Review/ReviewPanel';
import { WorkspaceList } from './components/Workspace/WorkspaceList';
import { WorkspaceDetail } from './components/Workspace/WorkspaceDetail';
import { Navbar } from './components/Layout/Navbar';
import { StatusBar } from './components/Layout/StatusBar';
import { useWebSocket } from './hooks/useWebSocket';
import { useStore } from './store';

function Dashboard() {
  const tasks = useStore((s) => s.tasks);
  const selectedTaskId = useStore((s) => s.selectedTaskId);
  const selectTask = useStore((s) => s.selectTask);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId);

  return (
    <div className="grid grid-cols-12 gap-6">
      <div className="col-span-8 space-y-4">
        <h2 className="text-lg font-semibold text-slate-400 mb-2 px-1">
          Pipeline Oversight
        </h2>
        {tasks.length === 0 ? (
          <div className="text-center py-20 text-slate-700">
            <p className="text-sm">No active pipelines.</p>
            <p className="text-xs mt-2">
              Click "NEW TASK" to start a planner session.
            </p>
          </div>
        ) : (
          tasks.map((task) => (
            <PipelineCard
              key={task.id}
              task={task}
              selected={selectedTaskId === task.id}
              onClick={() => selectTask(task.id)}
            />
          ))
        )}
      </div>
      <div className="col-span-4 space-y-6">
        <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 min-h-[400px]">
          {selectedTask ? (
            <div className="space-y-6">
              <div>
                <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3 flex items-center gap-2 underline decoration-indigo-500/50 underline-offset-4">
                  Active Specification
                </h4>
                <div className="text-xs text-slate-300 bg-black/40 p-4 rounded-xl border border-slate-800 leading-relaxed font-mono">
                  {selectedTask.spec || 'No spec generated yet.'}
                </div>
              </div>
              <div>
                <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3">
                  Live Telemetry
                </h4>
                <div className="space-y-2 max-h-60 overflow-y-auto">
                  {selectedTask.logs.map((log, i) => (
                    <div
                      key={i}
                      className="text-[10px] font-mono text-slate-500 flex gap-2"
                    >
                      <span className="text-indigo-600 shrink-0">&rsaquo;</span>
                      <span>{log}</span>
                    </div>
                  ))}
                  {selectedTask.status === 'Processing' && (
                    <div className="text-[10px] font-mono text-indigo-400 flex gap-2 animate-pulse">
                      <span className="shrink-0">&rsaquo;</span>
                      <span>
                        [{selectedTask.currentStage}] exploring workspace...
                      </span>
                    </div>
                  )}
                </div>
              </div>
            </div>
          ) : (
            <div className="h-full flex flex-col items-center justify-center text-slate-700 text-center">
              <ShieldCheck size={48} className="mb-4 opacity-10" />
              <p className="text-xs">
                Select a pipeline task to monitor agent activity.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function Workspaces() {
  return (
    <div className="grid grid-cols-12 gap-6">
      <div className="col-span-3">
        <WorkspaceList />
      </div>
      <div className="col-span-9">
        <WorkspaceDetail />
      </div>
    </div>
  );
}

export default function App() {
  useWebSocket();
  const currentView = useStore((s) => s.currentView);

  return (
    <div className="min-h-screen bg-[#07080a] text-slate-200 font-sans p-6 selection:bg-indigo-500/30">
      <Navbar />
      <main className="max-w-7xl mx-auto pb-20">
        {currentView === 'dashboard' && <Dashboard />}
        {currentView === 'workspaces' && <Workspaces />}
        {currentView === 'planner-chat' && <PlannerChat />}
        {currentView === 'review' && <ReviewPanel />}
      </main>
      <StatusBar />
    </div>
  );
}
