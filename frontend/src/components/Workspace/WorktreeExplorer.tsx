import { Bot, FileCode, FileText, GitBranch, Terminal } from 'lucide-react';
import { useStore } from '../../store';
import type { WorkspaceView } from '../../types';

interface Props {
  workspace: WorkspaceView;
}

export function WorktreeExplorer({ workspace }: Props) {
  const activeWorktreeId = useStore((s) => s.activeWorktreeId);
  const selectWorktree = useStore((s) => s.selectWorktree);

  const activeWorktree = workspace.worktrees.find(
    (wt) => wt.id === activeWorktreeId
  ) ?? workspace.worktrees[0];

  return (
    <div className="space-y-6">
      {/* Worktree selector */}
      <div>
        <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
          <GitBranch size={14} className="text-indigo-400" /> Active Worktrees
        </h4>
        <div className="space-y-2">
          {workspace.worktrees.map((wt) => (
            <div
              key={wt.id}
              onClick={() => selectWorktree(wt.id)}
              className={`p-3 rounded-lg border flex justify-between items-center cursor-pointer transition-all ${
                activeWorktreeId === wt.id
                  ? 'bg-indigo-500/10 border-indigo-500'
                  : 'bg-black/20 border-slate-800 hover:border-slate-700'
              }`}
            >
              <div className="flex items-center gap-3">
                <Terminal
                  size={14}
                  className={
                    activeWorktreeId === wt.id
                      ? 'text-indigo-400'
                      : 'text-slate-600'
                  }
                />
                <span
                  className={`text-[11px] font-mono ${
                    activeWorktreeId === wt.id
                      ? 'text-indigo-200'
                      : 'text-slate-400'
                  }`}
                >
                  {wt.branch}
                </span>
              </div>
              <span
                className={`text-[9px] font-bold uppercase ${
                  wt.status === 'Active'
                    ? 'text-emerald-500'
                    : wt.status === 'Processing'
                    ? 'text-amber-500'
                    : 'text-slate-600'
                }`}
              >
                {wt.status}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* File explorer */}
      <div>
        <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
          <Bot size={14} className="text-indigo-400" /> Agent-Generated Assets
        </h4>
        <div className="bg-black/60 border border-slate-800 rounded-xl overflow-hidden shadow-2xl">
          <div className="bg-slate-900 px-4 py-2 border-b border-slate-800 flex items-center justify-between">
            <span className="text-[10px] font-bold text-slate-500 uppercase tracking-tighter">
              Explorer: {activeWorktree?.branch ?? 'main'}
            </span>
            <div className="flex gap-1">
              <div
                className={`w-2 h-2 rounded-full ${
                  activeWorktree?.status === 'Processing'
                    ? 'bg-amber-500 animate-pulse'
                    : 'bg-slate-800'
                }`}
              />
              <div className="w-2 h-2 rounded-full bg-slate-800" />
            </div>
          </div>
          <div className="p-2 space-y-1 max-h-96 overflow-y-auto">
            {activeWorktree?.files && activeWorktree.files.length > 0 ? (
              activeWorktree.files.map((file) => (
                <div
                  key={file.name}
                  className="flex items-center justify-between p-2 hover:bg-indigo-500/10 rounded group cursor-pointer transition-all"
                >
                  <div className="flex items-center gap-3">
                    <FileCode size={16} className="text-indigo-400" />
                    <div>
                      <div className="text-[11px] font-mono text-slate-300">
                        {file.name}
                      </div>
                      <div className="text-[9px] text-slate-600 font-bold uppercase tracking-tighter">
                        Modified by {file.author}
                      </div>
                    </div>
                  </div>
                  <div className="text-[10px] font-mono text-slate-700 pr-2">
                    {file.size}
                  </div>
                </div>
              ))
            ) : (
              <div className="text-xs text-slate-600 italic p-8 border border-dashed border-slate-800/50 rounded-lg text-center m-2">
                No files currently in this worktree.
              </div>
            )}

            {activeWorktree?.status === 'Processing' && (
              <div className="flex items-center gap-3 p-3 border-t border-slate-800 mt-2 text-indigo-400/50 animate-pulse bg-indigo-500/5 rounded-b-lg">
                <FileText size={16} />
                <div className="text-[11px] font-mono italic">
                  Thinking: operating in {activeWorktree.branch} context...
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
