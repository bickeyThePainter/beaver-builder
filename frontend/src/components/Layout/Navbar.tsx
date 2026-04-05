import { Plus, ShieldCheck } from 'lucide-react';
import { useStore } from '../../store';
import type { ViewName } from '../../types';

const NAV_ITEMS: { id: ViewName; label: string }[] = [
  { id: 'dashboard', label: 'Pipeline' },
  { id: 'workspaces', label: 'Workspaces' },
  { id: 'review', label: 'Review' },
];

export function Navbar() {
  const currentView = useStore((s) => s.currentView);
  const setView = useStore((s) => s.setView);

  return (
    <nav className="flex justify-between items-center mb-10 border-b border-slate-800/50 pb-6 max-w-7xl mx-auto">
      <div className="flex items-center gap-8">
        <div
          className="flex items-center gap-2 cursor-pointer"
          onClick={() => setView('dashboard')}
        >
          <div className="w-8 h-8 bg-indigo-600 rounded flex items-center justify-center">
            <ShieldCheck size={20} className="text-white" />
          </div>
          <h1 className="text-xl font-black tracking-tighter text-white uppercase italic">
            Beaver
          </h1>
        </div>
        <div className="flex gap-10 text-[11px] font-bold uppercase tracking-widest">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              onClick={() => setView(item.id)}
              className={`flex items-center gap-2 transition-all ${
                currentView === item.id
                  ? 'text-indigo-400 border-b-2 border-indigo-500 pb-1'
                  : 'text-slate-600 hover:text-slate-300'
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      </div>
      <div className="flex items-center gap-4">
        <button
          onClick={() => setView('planner-chat')}
          className={`flex items-center gap-2 px-5 py-2.5 rounded-full font-bold text-xs transition-all ${
            currentView === 'planner-chat'
              ? 'bg-indigo-600 text-white shadow-lg shadow-indigo-500/30'
              : 'bg-slate-900 border border-slate-800 text-slate-400 hover:text-white'
          }`}
        >
          <Plus size={16} /> NEW TASK
        </button>
      </div>
    </nav>
  );
}
