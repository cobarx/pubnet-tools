import { describe, expect, test } from 'vitest';
import { checkReliability } from '../../src/checks/reliability.js';
import { checkTopology } from '../../src/checks/topology.js';

// spec: reliability-check-resilience#S1 (contract level: real `ping`, real gateway)
describe('checkReliability against the real network stack', () => {
  test('pings the real gateway and two external targets, reporting per-target shape', async () => {
    const topology = await checkTopology();
    const gatewayIp = topology.data?.gateway ?? null;

    const result = await checkReliability(gatewayIp);

    expect(result.status).not.toBe('failed');
    expect(result.data).not.toBeNull();

    const data = result.data!;
    expect(data.targets).toHaveLength(3);
    for (const target of data.targets) {
      expect(target.transmitted).toBe(10);
      expect(target.packetLossPct).toBeGreaterThanOrEqual(0);
      expect(target.packetLossPct).toBeLessThanOrEqual(100);
      if (target.reachable) {
        expect(target.rtts.length).toBeGreaterThan(0);
        expect(target.jitterMs).toBeGreaterThanOrEqual(0);
        expect(target.minMs).toBeLessThanOrEqual(target.avgMs!);
        expect(target.avgMs!).toBeLessThanOrEqual(target.maxMs!);
      }
    }
  }, 60_000);
});
