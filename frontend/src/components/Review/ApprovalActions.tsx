import { CheckCircle, XCircle } from 'lucide-react';
import { useState } from 'react';
import { useStore } from '../../store';

interface Props {
  pipelineId: string;
}

export function ApprovalActions({ pipelineId }: Props) {
  const [rejectReason, setRejectReason] = useState('');
  const [showReject, setShowReject] = useState(false);
  const sendOp = useStore((s) => s.sendOp);

  const handleApprove = () => {
    if (!sendOp) return;
    sendOp({
      type: 'ApproveHumanReview',
      payload: { pipeline_id: pipelineId },
    });
  };

  const handleReject = () => {
    if (!sendOp || !rejectReason.trim()) return;
    sendOp({
      type: 'RejectHumanReview',
      payload: { pipeline_id: pipelineId, reason: rejectReason },
    });
    setRejectReason('');
    setShowReject(false);
  };

  return (
    <div className="space-y-4">
      <div className="flex gap-3">
        <button
          onClick={handleApprove}
          className="flex-1 flex items-center justify-center gap-2 bg-emerald-600 hover:bg-emerald-500 text-white py-3 rounded-xl text-xs font-bold transition-all"
        >
          <CheckCircle size={16} /> APPROVE
        </button>
        <button
          onClick={() => setShowReject(!showReject)}
          className="flex-1 flex items-center justify-center gap-2 bg-red-600/80 hover:bg-red-500 text-white py-3 rounded-xl text-xs font-bold transition-all"
        >
          <XCircle size={16} /> REJECT
        </button>
      </div>

      {showReject && (
        <div className="space-y-2">
          <textarea
            value={rejectReason}
            onChange={(e) => setRejectReason(e.target.value)}
            placeholder="Explain why this should be rejected..."
            className="w-full bg-slate-900 border border-slate-800 rounded-xl p-3 text-sm text-slate-300 placeholder:text-slate-600 outline-none focus:border-red-500 resize-none h-24"
          />
          <button
            onClick={handleReject}
            disabled={!rejectReason.trim()}
            className="w-full bg-red-600 hover:bg-red-500 disabled:opacity-50 disabled:cursor-not-allowed text-white py-2 rounded-xl text-xs font-bold transition-all"
          >
            CONFIRM REJECTION
          </button>
        </div>
      )}
    </div>
  );
}
