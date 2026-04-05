import { FolderOpen, Plus } from 'lucide-react';
import { useStore } from '../../store';

export function WorkspaceList() {
  const workspaces = useStore((s) => s.workspaces);
  const selectedId = useStore((s) => s.selectedWorkspaceId);
  const selectWorkspace = useStore((s) => s.selectWorkspace);
  const tasks = useStore((s) => s.tasks);

  return (
    <div className="space-y-3">
      <h2 className="text-lg font-semibold text-slate-400 mb-4 px-1">
        Active Workspaces
      </h2>
      {workspaces.map((ws) => {
        const isSelected = selectedId === ws.id;
        const taskCount = tasks.filter((t) => t.workspaceId === ws.id).length;

        return (
          <div
            key={ws.id}
            onClick={() => selectWorkspace(ws.id)}
            className={`p-4 rounded-xl border cursor-pointer transition-all ${
              isSelected
                ? 'bg-indigo-600 border-indigo-500 shadow-lg shadow-indigo-500/20 text-white'
                : 'bg-slate-900/40 border-slate-800 text-slate-400 hover:border-slate-700'
            }`}
          >
            <div className="flex items-center gap-3">
              <FolderOpen
                size={18}
                className={isSelected ? 'text-indigo-100' : 'text-indigo-500'}
              />
              <div className="truncate">
                <div className="text-sm font-bold">{ws.name}</div>
                <div
                  className={`text-[10px] uppercase font-bold ${
                    isSelected ? 'text-indigo-200' : 'text-slate-600'
                  }`}
                >
                  {taskCount} Active Tasks
                </div>
              </div>
            </div>
          </div>
        );
      })}
      <button className="w-full py-3 rounded-xl border border-dashed border-slate-800 text-slate-600 flex items-center justify-center gap-2 hover:border-slate-600 hover:text-slate-400 transition-all text-xs font-bold uppercase mt-4">
        <Plus size={16} /> Provision Workspace
      </button>
    </div>
  );
}
