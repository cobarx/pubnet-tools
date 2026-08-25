import { describe, expect, test } from 'vitest';
import { checkSecurity } from '../../src/checks/security.js';
import { checkTopology } from '../../src/checks/topology.js';

// spec: dns-leak-detection, captive-portal-detection (contract level: real
// nmcli/resolvectl, real DoH providers, real canary endpoints). Asserts
// shape, not exact values — real networks vary.
describe('checkSecurity against the real network stack', () => {
  test('produces a full SecurityData shape from real WiFi, DNS, DoH, and canary probes', async () => {
    const topology = await checkTopology();
    const iface = topology.data?.interface ?? null;

    const result = await checkSecurity(iface);

    expect(result.status).not.toBe('failed');
    expect(result.data).not.toBeNull();

    const data = result.data!;
    expect(['WPA3', 'WPA2', 'WPA2-Enterprise', 'WPA', 'Open', 'Unknown']).toContain(
      data.encryption
    );
    expect(['clean', 'leaked', 'uncertain']).toContain(data.dnsLeak.verdict);
    expect(data.dnsLeak.probes.length).toBe(2);
    expect(['redirect', 'content-mismatch', 'none']).toContain(data.captivePortal.method);
    expect(typeof data.captivePortal.canaryUrl).toBe('string');
  }, 30_000);
});
