import { describe, expect, test, vi } from 'vitest';
import { checkTopology } from '../../src/checks/topology.js';
import type { ExecResult } from '../../src/utils/exec.js';

function execResult(stdout: string): ExecResult {
  return { stdout, stderr: '', exitCode: 0, timedOut: false };
}

describe('checkTopology', () => {
  // spec: topology-default-route-precondition#S2
  test('no default route is reported as skipped, and no further commands are attempted', async () => {
    const exec = vi.fn().mockResolvedValue(execResult(''));

    const result = await checkTopology(exec);

    expect(result.status).toBe('skipped');
    expect(result.data).toBeNull();
    expect(result.errors.length).toBeGreaterThan(0);
    expect(exec).toHaveBeenCalledTimes(1);
    expect(exec).toHaveBeenCalledWith(['ip', 'route', 'show', 'default']);
  });

  test('a default route drives the interface used for the addr and neigh lookups', async () => {
    const exec = vi
      .fn()
      .mockResolvedValueOnce(
        execResult('default via 192.168.5.1 dev wlan0 proto dhcp src 192.168.5.151 metric 600 ')
      )
      .mockResolvedValueOnce(execResult('    inet 192.168.5.151/24 brd 192.168.5.255 scope global wlan0'))
      .mockResolvedValueOnce(execResult('192.168.5.1 lladdr 68:7f:f0:55:77:7b REACHABLE '));

    const result = await checkTopology(exec);

    expect(result.status).toBe('ok');
    expect(result.data).toEqual({
      interface: 'wlan0',
      ipCidr: '192.168.5.151/24',
      gateway: '192.168.5.1',
      neighbors: [
        {
          ip: '192.168.5.1',
          mac: '68:7f:f0:55:77:7b',
          state: 'REACHABLE',
          device: 'wlan0',
          isGateway: true,
          vendor: 'TP-Link',
        },
      ],
      passiveNotice: 'Passive ARP cache — no active scan performed.',
    });
    expect(exec).toHaveBeenNthCalledWith(2, ['ip', 'addr', 'show', 'wlan0']);
    expect(exec).toHaveBeenNthCalledWith(3, ['ip', 'neigh', 'show', 'dev', 'wlan0']);
  });
});
