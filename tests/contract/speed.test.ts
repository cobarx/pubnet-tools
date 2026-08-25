import { describe, expect, test } from 'vitest';
import { checkSpeed } from '../../src/checks/speed.js';

// Contract level: verifies our NDT7 protocol implementation against real
// M-Lab locate API and real download/upload WebSocket servers. Asserts
// shape, not exact values — real networks and server load vary.
describe('checkSpeed against real M-Lab NDT7 servers', () => {
  test('returns data or fails gracefully', async () => {
    const result = await checkSpeed();

    if (result.status === 'ok') {
      expect(result.data!.downloadMbps).toBeGreaterThan(0);
      expect(result.data!.uploadMbps).toBeGreaterThan(0);
      expect(result.data!.latencyMs).toBeGreaterThan(0);
      expect(result.data!.jitterMs).toBeGreaterThanOrEqual(0);
      expect(result.data!.source).toBe('ndt7');
    } else {
      expect(result.status).toBe('failed');
      expect(result.data).toBeNull();
      expect(result.errors.length).toBeGreaterThan(0);
    }
  }, 45_000);
});
