import { Terminal } from 'lucide-react';
import type { TaskView } from '../../types';
import { StageIndicator } from './StageIndicator';

interface Props {
  task: TaskView;
  selected: boolean;
  onClick: () => void;
}

export function PipelineCard({ task, selected, onClick }: Props) {
  return (
    <div
      onClick={onClick}
      className={`group bg-slate-900/40 border ${
        selected
          ? 'border-indigo-500 ring-1 ring-indigo-500/20'
          : 'border-slate-800'
      } rounded-xl p-5 cursor-pointer transition-all`}
    >
      <div className="flex justify-between items-start mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400">
            <Terminal size={20} />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-mono text-slate-600">
                #{task.id}
              </span>
              {task.pipelineId && (
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-500 font-medium uppercase tracking-tighter">
                  {task.pipelineId}
                </span>
              )}
            </div>
            <h3 className="text-md font-bold text-white tracking-tight">
              {task.title}
            </h3>
          </div>
        </div>
        <span className="text-[10px] px-2 py-1 rounded border border-indigo-500/30 text-indigo-400 font-bold uppercase">
          {task.status}
        </span>
      </div>

      <StageIndicator currentStage={task.currentStage} />
    </div>
  );
}
