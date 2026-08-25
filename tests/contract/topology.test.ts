import { describe, expect, test } from 'vitest';
import { checkTopology } from '../../src/checks/topology.js';
import { isValidIPv4 } from '../../src/utils/network.js';

// spec: topology-default-route-precondition#S1, #S3
// Contract level: verifies our parsing assumptions against this machine's real
// `ip` output. Asserts shape, not exact values — real networks vary.
describe('checkTopology against the real network stack', () => {
  test('discovers the default interface, gateway, and ARP neighbors passively', async () => {
    const result = await checkTopology();

    expect(result.status).not.toBe('failed');
    expect(result.status).not.toBe('skipped');
    expect(result.data).not.toBeNull();

    const data = result.data!;
    expect(data.interface).toMatch(/^\S+$/);
    expect(isValidIPv4(data.gateway)).toBe(true);
    expect(data.ipCidr).toMatch(/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\/\d{1,2}$/);
    expect(Array.isArray(data.neighbors)).toBe(true);
    for (const neighbor of data.neighbors) {
      expect(isValidIPv4(neighbor.ip)).toBe(true);
      expect(neighbor.device).toBe(data.interface);
    }
    expect(data.passiveNotice).toBe('Passive ARP cache — no active scan performed.');
  });
});
