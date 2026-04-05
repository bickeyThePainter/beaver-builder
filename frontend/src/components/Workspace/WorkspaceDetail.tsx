import { Activity, Github } from 'lucide-react';
import { useStore } from '../../store';
import { WorktreeExplorer } from './WorktreeExplorer';

export function WorkspaceDetail() {
  const workspaces = useStore((s) => s.workspaces);
  const selectedId = useStore((s) => s.selectedWorkspaceId);

  const workspace = workspaces.find((ws) => ws.id === selectedId);

  if (!workspace) {
    return (
      <div className="text-center text-slate-600 py-20">
        Select a workspace from the sidebar.
      </div>
    );
  }

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-8 min-h-[600px]">
      <div className="flex justify-between items-start border-b border-slate-800 pb-6 mb-8">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="text-[10px] px-2 py-0.5 rounded bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 font-bold uppercase tracking-widest">
              Context ID: {workspace.id}
            </span>
          </div>
          <h1 className="text-2xl font-black text-white">{workspace.name}</h1>
        </div>
        <div className="flex gap-3">
          <button className="p-2 bg-slate-800 hover:bg-slate-700 rounded-lg text-slate-400 transition-all">
            <Github size={20} />
          </button>
          <button className="p-2 bg-slate-800 hover:bg-slate-700 rounded-lg text-slate-400 transition-all">
            <Activity size={20} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-8">
        {/* Left: Repos */}
        <div className="space-y-6">
          <div>
            <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
              <Github size={14} className="text-indigo-400" /> Linked
              Repositories
            </h4>
            <div className="space-y-2">
              {workspace.repos.length > 0 ? (
                workspace.repos.map((repo) => (
                  <div
                    key={repo}
                    className="flex items-center justify-between p-3 bg-black/40 border border-slate-800 rounded-lg group hover:border-indigo-500/50 transition-all"
                  >
                    <span className="text-xs font-mono text-indigo-300">
                      {repo}
                    </span>
                  </div>
                ))
              ) : (
                <div className="text-xs text-slate-600 italic p-4 border border-dashed border-slate-800 rounded-lg text-center">
                  No external repositories linked.
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Right: Worktree explorer */}
        <WorktreeExplorer workspace={workspace} />
      </div>
    </div>
  );
}
