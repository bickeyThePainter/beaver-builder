import React from 'react';
import { Github, Activity, Box, GitBranch, Terminal } from 'lucide-react';
import type { Workspace } from '../../types';
import { WorktreeExplorer } from './WorktreeExplorer';

interface Props {
  workspace: Workspace;
  activeWorktreeId: string | null;
  onSelectWorktree: (id: string) => void;
}

export const WorkspaceDetail: React.FC<Props> = ({ workspace, activeWorktreeId, onSelectWorktree }) => {
  const activeWorktree = workspace.worktrees.find((wt) => wt.id === activeWorktreeId) ?? workspace.worktrees[0];

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-8 min-h-[600px]">
      {/* Header */}
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
        {/* Left: Config */}
        <div className="space-y-6">
          {/* Repos */}
          <div>
            <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
              <Github size={14} className="text-indigo-400" /> Linked Repositories
            </h4>
            <div className="space-y-2">
              {workspace.repos.length > 0 ? (
                workspace.repos.map((repo) => (
                  <div
                    key={repo}
                    className="flex items-center justify-between p-3 bg-black/40 border border-slate-800 rounded-lg group hover:border-indigo-500/50 transition-all"
                  >
                    <span className="text-xs font-mono text-indigo-300">{repo}</span>
                  </div>
                ))
              ) : (
                <div className="text-xs text-slate-600 italic p-4 border border-dashed border-slate-800 rounded-lg text-center">
                  No external repositories linked.
                </div>
              )}
            </div>
          </div>

          {/* Swimlane */}
          <div>
            <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
              <Box size={14} className="text-indigo-400" /> Swimlane Environment
            </h4>
            <div className="grid grid-cols-1 gap-3">
              <div className="bg-slate-800/30 p-3 rounded-lg border border-slate-800 flex justify-between items-center">
                <div>
                  <div className="text-[9px] text-slate-500 uppercase font-bold mb-0.5">Active Swimlane</div>
                  <div className="text-xs text-indigo-300 font-mono">{workspace.swimlane.active || 'None'}</div>
                </div>
                {workspace.swimlane.active && (
                  <div className="px-2 py-1 rounded bg-emerald-500/10 border border-emerald-500/20 text-emerald-500 text-[9px] font-bold">
                    LIVE
                  </div>
                )}
              </div>
              <div className="bg-slate-800/30 p-3 rounded-lg border border-slate-800">
                <div className="text-[9px] text-slate-500 uppercase font-bold mb-0.5">Base Image</div>
                <div className="text-xs text-slate-400 font-mono">{workspace.swimlane.base || 'None'}</div>
              </div>
            </div>
          </div>

          {/* Worktrees */}
          <div>
            <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
              <GitBranch size={14} className="text-indigo-400" /> Active Worktrees
            </h4>
            <div className="space-y-2">
              {workspace.worktrees.map((wt) => (
                <div
                  key={wt.id}
                  onClick={() => onSelectWorktree(wt.id)}
                  className={`p-3 rounded-lg border flex justify-between items-center cursor-pointer transition-all ${
                    activeWorktreeId === wt.id
                      ? 'bg-indigo-500/10 border-indigo-500'
                      : 'bg-black/20 border-slate-800 hover:border-slate-700'
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <Terminal size={14} className={activeWorktreeId === wt.id ? 'text-indigo-400' : 'text-slate-600'} />
                    <span
                      className={`text-[11px] font-mono ${
                        activeWorktreeId === wt.id ? 'text-indigo-200' : 'text-slate-400'
                      }`}
                    >
                      {wt.branch}
                    </span>
                  </div>
                  <span
                    className={`text-[9px] font-bold uppercase ${
                      wt.status === 'active'
                        ? 'text-emerald-500'
                        : wt.status === 'processing'
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
        </div>

        {/* Right: Worktree Explorer */}
        <WorktreeExplorer worktree={activeWorktree ?? null} />
      </div>
    </div>
  );
};
