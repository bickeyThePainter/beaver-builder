import React from 'react';
import { Wand2, Rocket } from 'lucide-react';
import type { Spec } from '../../types';

interface Props {
  spec: Spec;
  onDeploy: () => void;
}

export const SpecCard: React.FC<Props> = ({ spec, onDeploy }) => {
  return (
    <div className="bg-indigo-500/5 border border-indigo-500/20 rounded-2xl p-6 space-y-4">
      <div className="flex justify-between items-center underline underline-offset-4 decoration-indigo-500/30">
        <h4 className="text-xs font-bold text-indigo-400 flex items-center gap-2">
          <Wand2 size={16} /> GENERATED SPECIFICATION
        </h4>
        <span className="text-[10px] text-indigo-400 font-bold tracking-tighter uppercase">Provisioning Ready</span>
      </div>
      <div className="space-y-3">
        <div className="text-sm font-bold text-white">{spec.title}</div>
        <p className="text-xs text-slate-400 italic">"{spec.description}"</p>
        <div className="flex flex-wrap gap-2">
          {spec.techStack.map((tag) => (
            <span key={tag} className="text-[10px] bg-slate-800 text-slate-400 px-2 py-1 rounded">
              {tag}
            </span>
          ))}
        </div>
      </div>
      <button
        onClick={onDeploy}
        className="w-full bg-indigo-600 hover:bg-indigo-500 text-white py-3 rounded-xl text-xs font-bold shadow-lg shadow-indigo-500/20 transition-all flex items-center justify-center gap-2"
      >
        DEPLOY PIPELINE <Rocket size={16} />
      </button>
    </div>
  );
};
