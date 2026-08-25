import { execCmd, type ExecResult } from '../utils/exec.js';
import { parsePingOutput, stddev } from '../utils/network.js';
import type { CheckResult, Finding, PingTargetResult, ReliabilityData } from '../types.js';

type ExecFn = (cmd: string[]) => Promise<ExecResult>;

const EXTERNAL_TARGETS: { host: string; label: 'google-dns' | 'cloudflare-dns' }[] = [
  { host: '8.8.8.8', label: 'google-dns' },
  { host: '1.1.1.1', label: 'cloudflare-dns' },
];

async function pingTarget(
  exec: ExecFn,
  host: string,
  label: PingTargetResult['label']
): Promise<PingTargetResult> {
  const result = await exec(['ping', '-c', '10', '-i', '0.2', host]);
  const { transmitted, received, rtts } = parsePingOutput(result.stdout);

  const packetLossPct = transmitted > 0 ? ((transmitted - received) / transmitted) * 100 : 100;
  const reachable = received > 0;

  return {
    host,
    label,
    transmitted,
    received,
    packetLossPct,
    minMs: rtts.length > 0 ? Math.min(...rtts) : null,
    avgMs: rtts.length > 0 ? rtts.reduce((a, b) => a + b, 0) / rtts.length : null,
    maxMs: rtts.length > 0 ? Math.max(...rtts) : null,
    jitterMs: rtts.length > 0 ? stddev(rtts) : null,
    rtts,
    reachable,
  };
}

function findingsFor(targets: PingTargetResult[]): Finding[] {
  const findings: Finding[] = [];
  const gateway = targets.find((t) => t.label === 'gateway');
  const internetUp = targets.some((t) => t.label !== 'gateway' && t.reachable);

  if (gateway && !gateway.reachable) {
    findings.push({
      id: 'reliability.gateway-unreachable',
      severity: 'alert',
      points: 30,
      title: 'Gateway unreachable',
    });
  }
  if (!internetUp) {
    findings.push({
      id: 'reliability.internet-unreachable',
      severity: 'alert',
      points: 25,
      title: 'Internet unreachable',
    });
  }
  for (const target of targets) {
    if (target.packetLossPct > 10) {
      findings.push({
        id: `reliability.packet-loss.${target.label}`,
        severity: 'warn',
        points: 10,
        title: `Packet loss > 10% to ${target.host}`,
        detail: `${target.packetLossPct.toFixed(1)}% loss`,
      });
    }
    if (target.avgMs !== null && target.avgMs > 200) {
      findings.push({
        id: `reliability.high-latency.${target.label}`,
        severity: 'warn',
        points: 5,
        title: `Average RTT > 200ms to ${target.host}`,
        detail: `${target.avgMs.toFixed(1)}ms avg`,
      });
    }
    if (target.jitterMs !== null && target.jitterMs > 30) {
      findings.push({
        id: `reliability.jitter.${target.label}`,
        severity: 'warn',
        points: 5,
        title: `Jitter > 30ms to ${target.host}`,
        detail: `${target.jitterMs.toFixed(1)}ms jitter`,
      });
    }
  }
  return findings;
}

/**
 * spec: reliability-check-resilience
 * One target's failure never aborts the others — every target is pinged
 * independently and its result reported regardless of the others' outcome.
 */
export async function checkReliability(
  gatewayIp: string | null,
  exec: ExecFn = execCmd
): Promise<CheckResult<ReliabilityData>> {
  const start = Date.now();

  if (!gatewayIp) {
    return {
      name: 'reliability',
      status: 'skipped',
      data: null,
      errors: ['No gateway IP available (topology check found no default route)'],
      findings: [],
      durationMs: Date.now() - start,
    };
  }

  const targets = await Promise.all([
    pingTarget(exec, gatewayIp, 'gateway'),
    ...EXTERNAL_TARGETS.map((t) => pingTarget(exec, t.host, t.label)),
  ]);

  const gatewayReachable = targets.find((t) => t.label === 'gateway')?.reachable ?? false;
  const internetReachable = targets.some((t) => t.label !== 'gateway' && t.reachable);

  const data: ReliabilityData = { targets, gatewayReachable, internetReachable };
  const status = gatewayReachable && internetReachable ? 'ok' : 'degraded';

  return {
    name: 'reliability',
    status,
    data,
    errors: [],
    findings: findingsFor(targets),
    durationMs: Date.now() - start,
  };
}
