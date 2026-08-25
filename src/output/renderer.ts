import chalk from 'chalk';
import type { ReliabilityData, Report, RiskLevel } from '../types.js';

function levelColor(level: RiskLevel) {
  switch (level) {
    case 'Low':
      return chalk.green;
    case 'Medium':
      return chalk.yellow;
    case 'High':
      return chalk.red;
  }
}

export interface HopSummary {
  lossPct: number;
  avgLatencyMs: number | null;
}

export interface ReliabilitySummary {
  local: HopSummary | null;
  internet: HopSummary | null;
}

/**
 * Condenses per-target ping data into the two things worth seeing at a
 * glance: is the local hop (gateway) lossy/slow, and is "the internet"
 * (the external targets, aggregated) lossy/slow. Per-target detail and
 * jitter stay in the JSON report — this is deliberately less than that,
 * not a different view of the same amount of information.
 */
export function summarizeReliability(rel: ReliabilityData): ReliabilitySummary {
  const gateway = rel.targets.find((t) => t.label === 'gateway') ?? null;
  const external = rel.targets.filter((t) => t.label !== 'gateway');

  const local: HopSummary | null = gateway
    ? { lossPct: gateway.packetLossPct, avgLatencyMs: gateway.avgMs }
    : null;

  let internet: HopSummary | null = null;
  if (external.length > 0) {
    const reachable = external.filter((t) => t.reachable && t.avgMs !== null);
    internet = {
      lossPct: Math.max(...external.map((t) => t.packetLossPct)),
      avgLatencyMs:
        reachable.length > 0
          ? reachable.reduce((sum, t) => sum + (t.avgMs ?? 0), 0) / reachable.length
          : null,
    };
  }

  return { local, internet };
}

function renderHop(label: string, hop: HopSummary | null): string {
  if (!hop) return `  ${label}: no data`;
  const latency = hop.avgLatencyMs !== null ? `${hop.avgLatencyMs.toFixed(1)}ms` : 'unreachable';
  return `  ${label}: ${hop.lossPct.toFixed(0)}% loss, ${latency}`;
}

/**
 * Findings drive the score but aren't rendered as their own list — every
 * finding restates something already visible in the sections below
 * (encryption, DNS leak verdict, captive portal, packet loss/latency), so
 * a separate list would just repeat it.
 */
function renderNetworkSection(report: Report): string[] {
  const lines: string[] = ['Network:'];
  const topo = report.topology.data;
  const sec = report.security.data;

  if (topo) {
    const gatewayVendor = topo.neighbors.find((n) => n.isGateway)?.vendor ?? null;
    const vendorSuffix = gatewayVendor ? ` (${gatewayVendor})` : '';
    lines.push(`  Interface: ${chalk.bold(topo.interface)} (${topo.ipCidr})`);
    lines.push(`  Gateway: ${topo.gateway}${vendorSuffix}`);
  } else {
    lines.push(`  Topology: ${report.topology.status}`);
  }

  if (sec) {
    lines.push(`  SSID: ${sec.ssid ?? 'no SSID'} — ${sec.encryption}`);
    if (sec.channel !== null) {
      const freq = sec.frequencyMhz !== null ? ` (${sec.frequencyMhz} MHz)` : '';
      const signal = sec.signalPercent !== null ? `, Signal: ${sec.signalPercent}%` : '';
      lines.push(`  Channel: ${sec.channel}${freq}${signal}`);
    }
  }

  return lines;
}

function renderSecuritySection(report: Report): string[] {
  const lines: string[] = ['Security:'];
  const sec = report.security.data;

  if (sec) {
    const wifiRisk = report.security.findings.find(
      (f) => f.id.startsWith('security.wifi-') && (f.severity === 'alert' || f.severity === 'warn')
    );
    if (wifiRisk) {
      const color = wifiRisk.severity === 'alert' ? chalk.red : chalk.yellow;
      lines.push(`  ${color(`⚠ ${wifiRisk.title}`)}`);
    }
    lines.push(`  DNS leak: ${sec.dnsLeak.verdict}`);
    lines.push(
      `  Captive portal: ${sec.captivePortal.detected ? `detected (${sec.captivePortal.method})` : 'none'}`
    );
  } else {
    lines.push(`  Security: ${report.security.status}`);
  }

  return lines;
}

function renderPerformanceSection(report: Report): string[] {
  const lines: string[] = ['Performance:'];
  const rel = report.reliability.data;
  const speed = report.speed.data;

  if (rel) {
    const { local, internet } = summarizeReliability(rel);
    lines.push(renderHop('Local', local));
    lines.push(renderHop('Internet', internet));
  } else {
    lines.push(`  Reliability: ${report.reliability.status}`);
  }

  if (speed) {
    lines.push(
      `  ${chalk.bold('Speed:')} ${speed.downloadMbps.toFixed(1)} Mbps down / ${speed.uploadMbps.toFixed(1)} Mbps up`
    );
  } else {
    lines.push(`  Speed: ${report.speed.status}`);
  }

  return lines;
}

export function renderReport(report: Report): string {
  const color = levelColor(report.score.level);
  const lines: string[] = [
    '',
    color.bold(`Risk: ${report.score.level} (${report.score.total} pts)`),
    '',
    ...renderNetworkSection(report),
    '',
    ...renderSecuritySection(report),
    '',
    ...renderPerformanceSection(report),
    '',
  ];

  return lines.join('\n');
}
