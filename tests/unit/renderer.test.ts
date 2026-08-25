import { describe, expect, test } from 'vitest';
import { renderReport, summarizeReliability } from '../../src/output/renderer.js';
import type { PingTargetResult, Report } from '../../src/types.js';

function target(overrides: Partial<PingTargetResult>): PingTargetResult {
  return {
    host: '1.1.1.1',
    label: 'cloudflare-dns',
    transmitted: 10,
    received: 10,
    packetLossPct: 0,
    minMs: 9,
    avgMs: 12.5,
    maxMs: 20,
    jitterMs: 2.1,
    rtts: [9, 12, 20],
    reachable: true,
    ...overrides,
  };
}

function baseReport(): Report {
  return {
    version: '0.1.0',
    timestamp: '2026-08-24T12:00:00.000Z',
    topology: {
      name: 'topology',
      status: 'ok',
      data: {
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
      },
      errors: [],
      findings: [],
      durationMs: 5,
    },
    security: {
      name: 'security',
      status: 'ok',
      data: {
        ssid: 'Berkeley-Visitor',
        encryption: 'Open',
        channel: 6,
        frequencyMhz: 2437,
        signalPercent: 80,
        dns: null,
        dnsLeak: { systemEgressIp: null, probes: [], leaked: false, verdict: 'uncertain' },
        captivePortal: { detected: false, method: 'none', redirectLocation: null, canaryUrl: 'x', httpStatus: 204 },
      },
      errors: [],
      findings: [{ id: 'security.wifi-open', severity: 'alert', points: 40, title: 'WiFi is open (unencrypted)' }],
      durationMs: 10,
    },
    reliability: {
      name: 'reliability',
      status: 'ok',
      data: {
        targets: [
          target({ host: '192.168.5.1', label: 'gateway', avgMs: 7.3, packetLossPct: 0 }),
          target({ host: '8.8.8.8', label: 'google-dns', avgMs: 13.2, packetLossPct: 0 }),
          target({ host: '1.1.1.1', label: 'cloudflare-dns', avgMs: 12.0, packetLossPct: 0 }),
        ],
        gatewayReachable: true,
        internetReachable: true,
      },
      errors: [],
      findings: [],
      durationMs: 2000,
    },
    speed: {
      name: 'speed',
      status: 'ok',
      data: { downloadMbps: 46.6, uploadMbps: 23.6, latencyMs: 23.1, jitterMs: 5.3, source: 'ndt7' },
      errors: [],
      findings: [],
      durationMs: 20000,
    },
    score: { total: 40, level: 'High', findings: [{ id: 'security.wifi-open', severity: 'alert', points: 40, title: 'WiFi is open (unencrypted)' }] },
  };
}

describe('summarizeReliability', () => {
  test('local is the gateway target directly', () => {
    const summary = summarizeReliability(baseReport().reliability.data!);
    expect(summary.local).toEqual({ lossPct: 0, avgLatencyMs: 7.3 });
  });

  test('internet aggregates the external targets: worst-case loss, average latency', () => {
    const rel = baseReport().reliability.data!;
    rel.targets = [
      target({ label: 'gateway', avgMs: 7.3, packetLossPct: 0 }),
      target({ label: 'google-dns', avgMs: 10, packetLossPct: 20 }),
      target({ label: 'cloudflare-dns', avgMs: 20, packetLossPct: 0 }),
    ];
    const summary = summarizeReliability(rel);
    expect(summary.internet).toEqual({ lossPct: 20, avgLatencyMs: 15 });
  });

  test('a fully unreachable hop reports its loss with a null latency, not a crash', () => {
    const rel = baseReport().reliability.data!;
    rel.targets = [
      target({ label: 'gateway', avgMs: null, packetLossPct: 100, reachable: false, jitterMs: null, minMs: null, maxMs: null }),
      target({ label: 'google-dns', avgMs: 10, packetLossPct: 0 }),
      target({ label: 'cloudflare-dns', avgMs: 20, packetLossPct: 0 }),
    ];
    const summary = summarizeReliability(rel);
    expect(summary.local).toEqual({ lossPct: 100, avgLatencyMs: null });
  });
});

describe('renderReport', () => {
  test('includes the risk level and score', () => {
    const output = renderReport(baseReport());
    expect(output).toContain('High');
    expect(output).toContain('40');
  });

  test('orders sections Network, then Security, then Performance', () => {
    const output = renderReport(baseReport());
    const networkIdx = output.indexOf('Network:');
    const securityIdx = output.indexOf('Security:');
    const perfIdx = output.indexOf('Performance:');

    expect(networkIdx).toBeGreaterThan(-1);
    expect(securityIdx).toBeGreaterThan(networkIdx);
    expect(perfIdx).toBeGreaterThan(securityIdx);
  });

  test('Network section has interface, gateway (with vendor), SSID/encryption, and channel/signal — no passive-ARP notice', () => {
    // spec/decision: the passiveNotice field still exists in the JSON report
    // (docs/decisions/2026-08-02-passive-topology.md's auditability
    // requirement holds there); this only drops it from the terminal view,
    // per docs/decisions/2026-08-25-passive-notice-terminal-only-in-json.md.
    //
    // SSID/encryption/channel/signal all come from the same passively-read
    // nmcli data — none of them are something conncheck actively checks —
    // so they group with the rest of the network-facts section rather than
    // with the checks Security actually performs (DNS leak, captive portal).
    const output = renderReport(baseReport());
    const lines = output.split('\n');
    const networkIdx = lines.indexOf('Network:');
    const securityIdx = lines.indexOf('Security:');

    expect(output).toContain('wlan0');
    expect(output).toContain('192.168.5.1');
    expect(output).toContain('TP-Link');
    expect(output).not.toContain('Passive ARP cache');

    const interfaceIdx = lines.findIndex((l) => l.includes('Interface:'));
    const gatewayIdx = lines.findIndex((l) => l.includes('Gateway:'));
    expect(gatewayIdx).toBe(interfaceIdx + 1);
    expect(lines[interfaceIdx]).not.toContain('gateway');
    expect(lines[gatewayIdx]).toContain('192.168.5.1');
    expect(lines[gatewayIdx]).toContain('TP-Link');

    const ssidIdx = lines.findIndex((l) => l.includes('SSID:'));
    expect(ssidIdx).toBeGreaterThan(networkIdx);
    expect(ssidIdx).toBeLessThan(securityIdx);
    expect(lines[ssidIdx]).toContain('Berkeley-Visitor');
    expect(lines[ssidIdx]).toContain('Open');

    const channelIdx = lines.findIndex((l) => l.includes('Channel:'));
    expect(channelIdx).toBe(ssidIdx + 1);
    expect(lines[channelIdx]).toContain('6');
    expect(lines[channelIdx]).toContain('2437');
    expect(lines[channelIdx]).toContain('80%');
  });

  test('Security section has only DNS leak and captive portal — no SSID/encryption/channel', () => {
    const output = renderReport(baseReport());
    expect(output).toContain('uncertain');

    const lines = output.split('\n');
    const securityIdx = lines.indexOf('Security:');
    const performanceIdx = lines.indexOf('Performance:');
    const securitySection = lines.slice(securityIdx, performanceIdx);
    expect(securitySection.some((l) => l.includes('SSID:'))).toBe(false);
    expect(securitySection.some((l) => l.includes('Channel:'))).toBe(false);
    expect(securitySection.some((l) => l.includes('Berkeley-Visitor'))).toBe(false);
  });

  test('Security calls out inadequate WiFi encryption using the existing wifi finding, not new classification logic', () => {
    // Network already shows the raw encryption value as a fact (SSID: ... —
    // Open); this is a distinct, deliberate risk assessment in Security,
    // not a duplicate of that — Security otherwise has nothing telling you
    // whether the encryption is actually a problem.
    const output = renderReport(baseReport());
    const lines = output.split('\n');
    const securityIdx = lines.indexOf('Security:');
    const performanceIdx = lines.indexOf('Performance:');
    const securitySection = lines.slice(securityIdx, performanceIdx);

    expect(securitySection.some((l) => l.includes('WiFi is open (unencrypted)'))).toBe(true);
  });

  test('Security has no WiFi risk callout when encryption is adequate (WPA3)', () => {
    const report = baseReport();
    report.security.findings = [
      { id: 'security.wifi-strong', severity: 'good', points: 0, title: 'WiFi uses WPA3' },
    ];
    const output = renderReport(report);
    const lines = output.split('\n');
    const securityIdx = lines.indexOf('Security:');
    const performanceIdx = lines.indexOf('Performance:');
    const securitySection = lines.slice(securityIdx, performanceIdx);

    expect(securitySection.some((l) => l.includes('WiFi'))).toBe(false);
  });

  test('does not repeat the DNS-leak or captive-portal findings\' title text — that information is already shown as their verdict/status', () => {
    const report = baseReport();
    report.security.findings = [
      { id: 'security.wifi-strong', severity: 'good', points: 0, title: 'WiFi uses WPA3' },
      { id: 'security.dns-leak', severity: 'alert', points: 25, title: 'DNS leak detected' },
    ];
    const output = renderReport(report);
    expect(output).not.toContain('DNS leak detected');
  });

  test('Performance section shows local vs internet loss/latency and speed, with no jitter or per-target detail', () => {
    const output = renderReport(baseReport());
    expect(output).toContain('Local');
    expect(output).toContain('Internet');
    expect(output).toContain('46.6');
    expect(output).toContain('23.6');
    expect(output).not.toContain('Jitter');
    expect(output).not.toContain('google-dns');
    expect(output).not.toContain('cloudflare-dns');
  });

  test('falls back to the check status when data is null (skipped/failed)', () => {
    const report = baseReport();
    report.topology = {
      name: 'topology',
      status: 'skipped',
      data: null,
      errors: ['No default route found'],
      findings: [],
      durationMs: 1,
    };
    const output = renderReport(report);
    expect(output).toContain('skipped');
  });
});
