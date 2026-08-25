import type { CheckStatus, Finding, RiskLevel, ScoreResult } from './types.js';

export interface ScorableResult {
  status: CheckStatus;
  findings: Finding[];
}

function levelFor(total: number): RiskLevel {
  if (total >= 50) return 'High';
  if (total >= 20) return 'Medium';
  return 'Low';
}

/**
 * spec: risk-scoring#S2 — a skipped check's findings never count, even if
 * one somehow carries points. Excluded here rather than trusted of every
 * check implementation.
 */
export function calculateScore(results: ScorableResult[]): ScoreResult {
  const findings = results
    .filter((r) => r.status !== 'skipped')
    .flatMap((r) => r.findings);

  const total = findings.reduce((sum, f) => sum + f.points, 0);

  return { total, level: levelFor(total), findings };
}
