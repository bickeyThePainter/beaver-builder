import { useBeaverStore } from '../store';
import { PIPELINE_STAGES } from '../types';

/**
 * Derived pipeline state for a given task.
 */
export function usePipelineProgress(taskId: string | null) {
  const task = useBeaverStore((s) => s.tasks.find((t) => t.id === taskId));

  if (!task) {
    return { stages: [], currentIndex: -1, task: null };
  }

  const currentIndex = PIPELINE_STAGES.indexOf(task.currentStage as typeof PIPELINE_STAGES[number]);

  const stages = PIPELINE_STAGES.map((id, idx) => ({
    id,
    isPast: idx < currentIndex,
    isCurrent: idx === currentIndex,
    isFuture: idx > currentIndex,
  }));

  return { stages, currentIndex, task };
}
