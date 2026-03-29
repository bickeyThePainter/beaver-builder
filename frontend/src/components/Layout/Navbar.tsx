import React from 'react';
import { ShieldCheck, Plus } from 'lucide-react';
import { useBeaverStore } from '../../store';

export const Navbar: React.FC = () => {
  const currentView = useBeaverStore((s) => s.currentView);
  const setCurrentView = useBeaverStore((s) => s.setCurrentView);

  return (
    <nav className="flex justify-between items-center mb-10 border-b border-slate-800/50 pb-6 max-w-7xl mx-auto">
      <div className="flex items-center gap-8">
        <div className="flex items-center gap-2 cursor-pointer" onClick={() => setCurrentView('dashboard')}>
          <div className="w-8 h-8 bg-indigo-600 rounded flex items-center justify-center">
            <ShieldCheck size={20} className="text-white" />
          </div>
          <h1 className="text-xl font-black tracking-tighter text-white uppercase italic">Beaver</h1>
        </div>
        <div className="flex gap-10 text-[11px] font-bold uppercase tracking-widest">
          <button
            onClick={() => setCurrentView('dashboard')}
            className={`flex items-center gap-2 transition-all ${
              currentView === 'dashboard'
                ? 'text-indigo-400 border-b-2 border-indigo-500 pb-1'
                : 'text-slate-600 hover:text-slate-300'
            }`}
          >
            Pipeline
          </button>
          <button
            onClick={() => setCurrentView('workspaces')}
            className={`flex items-center gap-2 transition-all ${
              currentView === 'workspaces'
                ? 'text-indigo-400 border-b-2 border-indigo-500 pb-1'
                : 'text-slate-600 hover:text-slate-300'
            }`}
          >
            Workspaces
          </button>
        </div>
      </div>
      <div className="flex items-center gap-4">
        <button
          onClick={() => setCurrentView('intent-chat')}
          className={`flex items-center gap-2 px-5 py-2.5 rounded-full font-bold text-xs transition-all ${
            currentView === 'intent-chat'
              ? 'bg-indigo-600 text-white shadow-lg shadow-indigo-500/30'
              : 'bg-slate-900 border border-slate-800 text-slate-400 hover:text-white'
          }`}
        >
          <Plus size={16} /> NEW TASK
        </button>
      </div>
    </nav>
  );
};
