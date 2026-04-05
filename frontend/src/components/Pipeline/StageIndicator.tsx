import { CheckCircle } from 'lucide-react';
import { PIPELINE_STAGES, STAGE_LABELS, type Stage } from '../../types';
import { stageIndex } from '../../hooks/usePipeline';

interface Props {
  currentStage: Stage;
}

export function StageIndicator({ currentStage }: Props) {
  const current = stageIndex(currentStage);
  const isCompleted = currentStage === 'completed';

  return (
    <div className="grid grid-cols-7 gap-1 pt-2">
      {PIPELINE_STAGES.map((stage, idx) => {
        const isPast = isCompleted || current > idx;
        const isCurrent = current === idx && !isCompleted;

        return (
          <div key={stage} className="relative">
            <div
              className={`h-1 rounded-full mb-2 ${
                isPast
                  ? 'bg-indigo-500'
                  : isCurrent
                  ? 'bg-indigo-400 animate-pulse'
                  : 'bg-slate-800'
              }`}
            />
            <div
              className={`flex flex-col items-center gap-1 ${
                isCurrent
                  ? 'text-indigo-300'
                  : isPast
                  ? 'text-slate-400'
                  : 'text-slate-700'
              }`}
            >
              <span
                className={`p-1 rounded-md border ${
                  isCurrent
                    ? 'border-indigo-500 bg-indigo-500/10'
                    : 'border-transparent'
                }`}
              >
                {isPast ? <CheckCircle size={10} /> : <div className="w-2.5 h-2.5 rounded-full bg-current" />}
              </span>
              <span className="text-[8px] font-bold uppercase whitespace-nowrap">
                {STAGE_LABELS[stage]}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
