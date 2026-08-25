import { execCmd, type ExecResult } from '../utils/exec.js';
import { parseIpAddr, parseIpNeigh, parseIpRoute } from '../utils/network.js';
import type { CheckResult, TopologyData } from '../types.js';

const PASSIVE_NOTICE = 'Passive ARP cache — no active scan performed.';

type ExecFn = (cmd: string[]) => Promise<ExecResult>;

/**
 * spec: topology-default-route-precondition
 * Sequential: default route determines the interface every other lookup
 * (and every downstream check's gateway) depends on.
 */
export async function checkTopology(exec: ExecFn = execCmd): Promise<CheckResult<TopologyData>> {
  const start = Date.now();
  const routeResult = await exec(['ip', 'route', 'show', 'default']);
  const route = parseIpRoute(routeResult.stdout);

  if (!route) {
    return {
      name: 'topology',
      status: 'skipped',
      data: null,
      errors: ['No default route found'],
      findings: [],
      durationMs: Date.now() - start,
    };
  }

  const [addrResult, neighResult] = await Promise.all([
    exec(['ip', 'addr', 'show', route.device]),
    exec(['ip', 'neigh', 'show', 'dev', route.device]),
  ]);

  const addr = parseIpAddr(addrResult.stdout);
  const neighbors = parseIpNeigh(neighResult.stdout, route.device, route.gateway);

  const errors: string[] = [];
  if (!addr) errors.push(`Could not determine IP address for ${route.device}`);

  const data: TopologyData = {
    interface: route.device,
    ipCidr: addr ? `${addr.ip}/${addr.prefix}` : '',
    gateway: route.gateway,
    neighbors,
    passiveNotice: PASSIVE_NOTICE,
  };

  return {
    name: 'topology',
    status: addr ? 'ok' : 'degraded',
    data,
    errors,
    findings: [],
    durationMs: Date.now() - start,
  };
}
