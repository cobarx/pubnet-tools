import { describe, expect, test } from 'vitest';
import { calculateScore } from '../../src/scoring.js';
import type { CheckStatus, Finding } from '../../src/types.js';

function finding(points: number, id = 'test.finding'): Finding {
  return { id, severity: points === 0 ? 'good' : 'warn', points, title: id };
}

function result(status: CheckStatus, findings: Finding[]) {
  return { status, findings };
}

describe('calculateScore', () => {
  // spec: risk-scoring#S1
  test('zero-point findings across all checks score Low', () => {
    const score = calculateScore([
      result('ok', [finding(0), finding(0)]),
      result('ok', [finding(0)]),
    ]);
    expect(score.total).toBe(0);
    expect(score.level).toBe('Low');
  });

  // spec: risk-scoring#S2
  test('a skipped check contributes nothing, even if it carries findings', () => {
    const score = calculateScore([
      result('skipped', [finding(40, 'should-not-count')]),
      result('ok', [finding(0)]),
    ]);
    expect(score.total).toBe(0);
    expect(score.level).toBe('Low');
    expect(score.findings.some((f) => f.id === 'should-not-count')).toBe(false);
  });

  // spec: risk-scoring#S3
  test('19 points is Low, 20 points is Medium', () => {
    const low = calculateScore([result('ok', [finding(19)])]);
    expect(low.level).toBe('Low');

    const medium = calculateScore([result('ok', [finding(20)])]);
    expect(medium.level).toBe('Medium');
  });

  // spec: risk-scoring#S4
  test('49 points is Medium, 50 points is High', () => {
    const medium = calculateScore([result('ok', [finding(49)])]);
    expect(medium.level).toBe('Medium');

    const high = calculateScore([result('ok', [finding(50)])]);
    expect(high.level).toBe('High');
  });

  test('is a pure function: identical input produces identical output', () => {
    const input = [result('ok', [finding(15)]), result('degraded', [finding(10)])];
    expect(calculateScore(input)).toEqual(calculateScore(input));
  });

  test('sums points across multiple non-skipped checks', () => {
    const score = calculateScore([
      result('ok', [finding(10, 'a')]),
      result('degraded', [finding(15, 'b')]),
      result('failed', [finding(5, 'c')]),
    ]);
    expect(score.total).toBe(30);
    expect(score.level).toBe('Medium');
  });
});
