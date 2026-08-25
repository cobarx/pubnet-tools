import { beforeEach, describe, expect, test, vi } from 'vitest';

const oraFactory = vi.fn();
vi.mock('ora', () => ({ default: oraFactory }));

const checkTopology = vi.fn();
vi.mock('../../src/checks/topology.js', () => ({ checkTopology }));
const checkSecurity = vi.fn();
vi.mock('../../src/checks/security.js', () => ({ checkSecurity }));
const checkReliability = vi.fn();
vi.mock('../../src/checks/reliability.js', () => ({ checkReliability }));
const checkSpeed = vi.fn();
vi.mock('../../src/checks/speed.js', () => ({ checkSpeed }));

const { runAudit } = await import('../../src/cli.js');

function okResult(name: string) {
  return { name, status: 'ok' as const, data: null, errors: [], findings: [], durationMs: 1 };
}

// ora explicitly warns when more than one of its spinner instances is
// live on the same TTY stream at once ("Multiple concurrent spinners
// detected") — but that warning is gated behind `stream.isTTY`, so it
// can never fire in a piped test subprocess and can't be asserted against
// at the workflow level. This tests our own concurrency discipline
// directly instead: at most one spinner should ever be active at a time,
// regardless of how many checks run concurrently.
describe('runAudit spinner discipline', () => {
  let activeCount = 0;
  let maxConcurrent = 0;
  let finishCall: { method: string; text: string } | null = null;

  beforeEach(() => {
    activeCount = 0;
    maxConcurrent = 0;
    finishCall = null;
    oraFactory.mockReset();
    oraFactory.mockImplementation(() => {
      const spinner = {
        start: vi.fn(() => {
          activeCount++;
          maxConcurrent = Math.max(maxConcurrent, activeCount);
          return spinner;
        }),
        succeed: vi.fn((text: string) => {
          activeCount--;
          finishCall = { method: 'succeed', text };
          return spinner;
        }),
        warn: vi.fn((text: string) => {
          activeCount--;
          finishCall = { method: 'warn', text };
          return spinner;
        }),
        fail: vi.fn((text: string) => {
          activeCount--;
          finishCall = { method: 'fail', text };
          return spinner;
        }),
      };
      return spinner;
    });

    checkTopology.mockResolvedValue({
      ...okResult('topology'),
      data: { interface: 'wlan0', ipCidr: '1.2.3.4/24', gateway: '1.2.3.1', neighbors: [], passiveNotice: '' },
    });
    checkSecurity.mockResolvedValue(okResult('security'));
    checkReliability.mockResolvedValue(okResult('reliability'));
    checkSpeed.mockResolvedValue(okResult('speed'));
  });

  test('never has more than one spinner active at a time, even with three checks running concurrently', async () => {
    await runAudit();
    expect(maxConcurrent).toBeLessThanOrEqual(1);
  });

  test('uses exactly one spinner instance for the whole run, not one per phase', async () => {
    await runAudit();
    expect(oraFactory).toHaveBeenCalledTimes(1);
  });

  test('when every check is ok, the final message is terse rather than listing all four as ok', async () => {
    await runAudit();
    expect(finishCall?.method).toBe('succeed');
    expect(finishCall?.text).not.toContain('Topology: ok');
    expect(finishCall?.text).not.toContain('Security: ok');
  });

  test('when one check has an issue, only that check is named — the passing ones are not', async () => {
    checkReliability.mockResolvedValue({ ...okResult('reliability'), status: 'degraded' });
    await runAudit();
    expect(finishCall?.method).toBe('warn');
    expect(finishCall?.text).toContain('Reliability: degraded');
    expect(finishCall?.text).not.toContain('Topology');
    expect(finishCall?.text).not.toContain('Security: ok');
    expect(finishCall?.text).not.toContain('Speed: ok');
  });
});
