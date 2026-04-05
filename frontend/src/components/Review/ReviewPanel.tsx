import { ShieldCheck } from 'lucide-react';
import { useStore } from '../../store';
import { ApprovalActions } from './ApprovalActions';
import { DiffView } from './DiffView';

export function ReviewPanel() {
  const reviewPipelineId = useStore((s) => s.reviewPipelineId);
  const reviewSummary = useStore((s) => s.reviewSummary);
  const tasks = useStore((s) => s.tasks);

  const task = tasks.find((t) => t.pipelineId === reviewPipelineId);

  if (!reviewPipelineId) {
    return (
      <div className="flex flex-col items-center justify-center h-[60vh] text-slate-700 text-center">
        <ShieldCheck size={64} className="mb-6 opacity-10" />
        <p className="text-sm text-slate-500">No pipeline awaiting human review.</p>
        <p className="text-xs text-slate-600 mt-2">
          When a pipeline reaches the Human Review stage, it will appear here.
        </p>
      </div>
    );
  }

  // Placeholder diff data — in production this would come from git_ops
  const files = [
    { name: 'src/main.rs', additions: 42, deletions: 8 },
    { name: 'src/lib.rs', additions: 15, deletions: 3 },
    { name: 'tests/integration.rs', additions: 88, deletions: 0 },
  ];

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-8">
        <div className="flex items-center justify-between mb-6 pb-4 border-b border-slate-800">
          <div>
            <h2 className="text-lg font-bold text-white">Human Review</h2>
            <p className="text-xs text-slate-500 mt-1">
              Pipeline: {reviewPipelineId}
              {task && ` | Task: ${task.title}`}
            </p>
          </div>
          <span className="text-[10px] px-3 py-1 rounded-full bg-amber-500/10 border border-amber-500/20 text-amber-400 font-bold uppercase">
            Awaiting Decision
          </span>
        </div>

        {/* Summary */}
        {reviewSummary && (
          <div className="mb-6">
            <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3">
              Review Summary
            </h4>
            <div className="text-sm text-slate-300 bg-black/40 p-4 rounded-xl border border-slate-800 leading-relaxed">
              {reviewSummary}
            </div>
          </div>
        )}

        {/* Diff */}
        <div className="mb-6">
          <h4 className="text-[10px] font-bold text-slate-500 uppercase mb-3">
            Files Changed
          </h4>
          <DiffView files={files} />
        </div>

        {/* Actions */}
        <ApprovalActions pipelineId={reviewPipelineId} />
      </div>
    </div>
  );
}
