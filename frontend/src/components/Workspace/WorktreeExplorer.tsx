import React from 'react';
import { Bot, FileCode, FileText } from 'lucide-react';
import type { Worktree } from '../../types';

interface Props {
  worktree: Worktree | null;
}

export const WorktreeExplorer: React.FC<Props> = ({ worktree }) => {
  return (
    <div>
      <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest mb-4 flex items-center gap-2">
        <Bot size={14} className="text-indigo-400" /> Agent-Generated Assets
      </h4>
      <div className="bg-black/60 border border-slate-800 rounded-xl overflow-hidden shadow-2xl">
        <div className="bg-slate-900 px-4 py-2 border-b border-slate-800 flex items-center justify-between">
          <span className="text-[10px] font-bold text-slate-500 uppercase tracking-tighter">
            Explorer: {worktree?.branch ?? 'none'}
          </span>
          <div className="flex gap-1">
            <div
              className={`w-2 h-2 rounded-full ${
                worktree?.status === 'processing' ? 'bg-amber-500 animate-pulse' : 'bg-slate-800'
              }`}
            />
            <div className="w-2 h-2 rounded-full bg-slate-800" />
          </div>
        </div>
        <div className="p-2 space-y-1 max-h-96 overflow-y-auto no-scrollbar">
          {worktree?.files && worktree.files.length > 0 ? (
            worktree.files.map((file) => (
              <div
                key={file.name}
                className="flex items-center justify-between p-2 hover:bg-indigo-500/10 rounded group cursor-pointer transition-all"
              >
                <div className="flex items-center gap-3">
                  <FileCode size={16} className="text-indigo-400" />
                  <div>
                    <div className="text-[11px] font-mono text-slate-300">{file.name}</div>
                    <div className="text-[9px] text-slate-600 font-bold uppercase tracking-tighter">
                      Modified by {file.author}
                    </div>
                  </div>
                </div>
                <div className="text-[10px] font-mono text-slate-700 pr-2">{file.size}</div>
              </div>
            ))
          ) : (
            <div className="text-xs text-slate-600 italic p-8 border border-dashed border-slate-800/50 rounded-lg text-center m-2">
              No files in this worktree.
            </div>
          )}

          {worktree?.status === 'processing' && (
            <div className="flex items-center gap-3 p-3 border-t border-slate-800 mt-2 text-indigo-400/50 animate-pulse bg-indigo-500/5 rounded-b-lg">
              <FileText size={16} />
              <div className="text-[11px] font-mono italic">Thinking: operating in {worktree.branch} context...</div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
