import React from 'react';
import { CheckCircle, Wand2, FolderOpen, FileText, Code2, Search, User, Rocket, GitPullRequest } from 'lucide-react';
import { STAGE_INFO } from '../../types';
import type { StageId } from '../../types';

const STAGE_ICONS: Record<StageId, React.ReactNode> = {
  intent_clarifier: <Wand2 size={10} />,
  init_agent: <FolderOpen size={10} />,
  planner: <FileText size={10} />,
  coder: <Code2 size={10} />,
  reviewer: <Search size={10} />,
  human_review: <User size={10} />,
  deploy: <Rocket size={10} />,
  push: <GitPullRequest size={10} />,
};

interface Props {
  currentStage: string;
}

export const StageIndicator: React.FC<Props> = ({ currentStage }) => {
  const currentIdx = STAGE_INFO.findIndex((s) => s.id === currentStage);

  return (
    <div className="grid grid-cols-8 gap-1 pt-2">
      {STAGE_INFO.map((stage, idx) => {
        const isPast = idx < currentIdx;
        const isCurrent = idx === currentIdx;

        return (
          <div key={stage.id} className="relative">
            <div
              className={`h-1 rounded-full mb-2 ${
                isPast ? 'bg-indigo-500' : isCurrent ? 'bg-indigo-400 animate-pulse' : 'bg-slate-800'
              }`}
            />
            <div
              className={`flex flex-col items-center gap-1 ${
                isCurrent ? 'text-indigo-300' : isPast ? 'text-slate-400' : 'text-slate-700'
              }`}
            >
              <span
                className={`p-1 rounded-md border ${
                  isCurrent ? 'border-indigo-500 bg-indigo-500/10' : 'border-transparent'
                }`}
              >
                {isPast ? <CheckCircle size={10} /> : STAGE_ICONS[stage.id]}
              </span>
              <span className="text-[8px] font-bold uppercase whitespace-nowrap">{stage.label}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
};
