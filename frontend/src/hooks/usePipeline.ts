import { PIPELINE_STAGES, type Stage } from '../types';

/** Returns the 0-based index of a stage in the 7-stage pipeline bar, or -1. */
export function stageIndex(stage: Stage): number {
  return PIPELINE_STAGES.indexOf(stage);
}

/** Returns progress percentage (0-100) for a stage. */
export function stageProgress(stage: Stage): number {
  if (stage === 'completed') return 100;
  if (stage === 'failed' || stage === 'created') return 0;
  const idx = stageIndex(stage);
  if (idx < 0) return 0;
  return Math.round(((idx + 1) / PIPELINE_STAGES.length) * 100);
}
