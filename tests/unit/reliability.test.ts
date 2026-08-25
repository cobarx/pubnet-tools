import { describe, expect, test, vi } from 'vitest';
import { checkReliability } from '../../src/checks/reliability.js';
import type { ExecResult } from '../../src/utils/exec.js';

function pingOutput(transmitted: number, received: number, rtts: number[]): string {
  const packetLines = rtts.map((t, i) => `64 bytes from x: icmp_seq=${i + 1} time=${t} ms`);
  return [
    ...packetLines,
    `${transmitted} packets transmitted, ${received} received, 0% packet loss`,
  ].join('\n');
}

function execResult(stdout: string): ExecResult {
  return { stdout, stderr: '', exitCode: 0, timedOut: false };
}

const REACHABLE = execResult(pingOutput(10, 10, [10, 12, 11, 9, 10, 11, 10, 12, 9, 11]));
const UNREACHABLE = execResult(pingOutput(10, 0, []));

describe('checkReliability', () => {
  // spec: reliability-check-resilience#S4
  test('no gateway IP means no pings are attempted', async () => {
    const exec = vi.fn();

    const result = await checkReliability(null, exec);

    expect(result.status).toBe('skipped');
    expect(result.data).toBeNull();
    expect(exec).not.toHaveBeenCalled();
  });

  // spec: reliability-check-resilience#S1
  test('all three targets reachable is ok, with both reachable flags true', async () => {
    const exec = vi.fn().mockResolvedValue(REACHABLE);

    const result = await checkReliability('192.168.5.1', exec);

    expect(result.status).toBe('ok');
    expect(result.data!.gatewayReachable).toBe(true);
    expect(result.data!.internetReachable).toBe(true);
    expect(result.data!.targets).toHaveLength(3);
    for (const target of result.data!.targets) {
      expect(target.transmitted).toBe(10);
      expect(target.reachable).toBe(true);
      expect(target.jitterMs).not.toBeNull();
    }
  });

  // spec: reliability-check-resilience#S2
  test('gateway down, internet up: still degraded, not aborted', async () => {
    const exec = vi.fn().mockImplementation(async (cmd: string[]) => {
      const host = cmd[cmd.length - 1];
      return host === '192.168.5.1' ? UNREACHABLE : REACHABLE;
    });

    const result = await checkReliability('192.168.5.1', exec);

    expect(result.status).toBe('degraded');
    expect(result.data!.gatewayReachable).toBe(false);
    expect(result.data!.internetReachable).toBe(true);
    expect(result.data!.targets).toHaveLength(3);
    const gatewayTarget = result.data!.targets.find((t) => t.label === 'gateway')!;
    expect(gatewayTarget.packetLossPct).toBe(100);
    expect(gatewayTarget.reachable).toBe(false);
  });

  // spec: reliability-check-resilience#S3
  test('no target reachable: three real bad results, degraded not failed', async () => {
    const exec = vi.fn().mockResolvedValue(UNREACHABLE);

    const result = await checkReliability('192.168.5.1', exec);

    expect(result.status).toBe('degraded');
    expect(result.data!.gatewayReachable).toBe(false);
    expect(result.data!.internetReachable).toBe(false);
    expect(result.data!.targets).toHaveLength(3);
    for (const target of result.data!.targets) {
      expect(target.packetLossPct).toBe(100);
      expect(target.reachable).toBe(false);
    }
  });
});
