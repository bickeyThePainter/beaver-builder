import { useStore } from '../../store';

export function StatusBar() {
  const connected = useStore((s) => s.connected);
  const workspaces = useStore((s) => s.workspaces);
  const tasks = useStore((s) => s.tasks);

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 bg-slate-900/80 backdrop-blur-md border border-slate-800 px-6 py-2.5 rounded-full flex gap-8 items-center shadow-2xl z-50">
      <div className="flex items-center gap-2">
        <div
          className={`w-2 h-2 rounded-full ${
            connected ? 'bg-emerald-500 animate-pulse' : 'bg-red-500'
          }`}
        />
        <span className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">
          {connected ? 'Orchestrator Online' : 'Disconnected'}
        </span>
      </div>
      <div className="flex items-center gap-2 border-l border-slate-800 pl-8">
        <span className="text-[10px] font-bold text-indigo-400 uppercase tracking-widest">
          Contexts: {workspaces.length}
        </span>
      </div>
      <div className="flex items-center gap-2 border-l border-slate-800 pl-8">
        <span className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">
          Tasks: {tasks.length}
        </span>
      </div>
    </div>
  );
}
