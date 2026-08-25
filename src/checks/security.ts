import axios from 'axios';
import { execCmd, type ExecResult } from '../utils/exec.js';
import { extractRemoteIp, ipFamily, parseNmcliWifi, parseResolvectlStatus } from '../utils/network.js';
import type {
  CaptivePortalResult,
  CheckResult,
  DnsLeakResult,
  DohProbe,
  Finding,
  SecurityData,
  WifiEncryption,
} from '../types.js';

type ExecFn = (cmd: string[]) => Promise<ExecResult>;

const DOH_TIMEOUT_MS = 8000;
const CAPTIVE_TIMEOUT_MS = 5000;

// ---------------------------------------------------------------------------
// DNS leak — spec: dns-leak-detection
// ---------------------------------------------------------------------------

export interface RawDohProbe {
  provider: 'cloudflare' | 'google';
  reachable: boolean;
  egressIp: string | null;
}

function sameSlash24(a: string, b: string): boolean {
  return a.split('.').slice(0, 3).join('.') === b.split('.').slice(0, 3).join('.');
}

/**
 * spec: dns-leak-detection#S1-S5
 * Only IPv4-vs-IPv4 pairs are ever comparable (see
 * docs/decisions/2026-08-24-dns-leak-address-family-matching.md) — a
 * family-mismatched or IPv6-vs-IPv6 pair counts as neither agreement nor
 * disagreement, never as a false leak or a false clean.
 */
export function classifyDnsLeak(
  systemEgressIp: string | null,
  rawProbes: RawDohProbe[]
): DnsLeakResult {
  const probes: DohProbe[] = rawProbes.map((p) => ({
    provider: p.provider,
    egressIp: p.egressIp,
    reachable: p.reachable,
  }));

  let anyComparable = false;
  let anyDisagree = false;

  if (systemEgressIp && ipFamily(systemEgressIp) === 'v4') {
    for (const p of rawProbes) {
      if (!p.reachable || !p.egressIp) continue;
      if (ipFamily(p.egressIp) !== 'v4') continue;
      anyComparable = true;
      if (!sameSlash24(systemEgressIp, p.egressIp)) anyDisagree = true;
    }
  }

  const verdict: DnsLeakResult['verdict'] = anyDisagree
    ? 'leaked'
    : anyComparable
      ? 'clean'
      : 'uncertain';

  return { systemEgressIp, probes, leaked: verdict === 'leaked', verdict };
}

async function getSystemEgressIp(exec: ExecFn): Promise<string | null> {
  const result = await exec(['resolvectl', 'query', '--type=TXT', 'whoami.cloudflare.com']);
  return extractRemoteIp(result.stdout);
}

async function probeDoh(provider: 'cloudflare' | 'google'): Promise<RawDohProbe> {
  const url =
    provider === 'cloudflare'
      ? 'https://cloudflare-dns.com/dns-query?name=whoami.cloudflare.com&type=TXT'
      : 'https://dns.google/resolve?name=whoami.cloudflare.com&type=TXT';

  try {
    const res = await axios.get(url, {
      ...(provider === 'cloudflare' ? { headers: { accept: 'application/dns-json' } } : {}),
      timeout: DOH_TIMEOUT_MS,
      validateStatus: () => true,
    });
    if (res.status !== 200) return { provider, reachable: false, egressIp: null };
    const egressIp = extractRemoteIp(JSON.stringify(res.data));
    return { provider, reachable: egressIp !== null, egressIp };
  } catch {
    return { provider, reachable: false, egressIp: null };
  }
}

// ---------------------------------------------------------------------------
// Captive portal — spec: captive-portal-detection
// ---------------------------------------------------------------------------

export interface CanaryResponse {
  status: number | null;
  location: string | null;
  body: string;
}

export interface CanaryExpectation {
  expectedStatus: number;
  expectedBodyContains?: string;
}

export type CaptivePortalClassification = Omit<CaptivePortalResult, 'canaryUrl'>;

/** spec: captive-portal-detection#S1-S3 */
export function classifyCaptivePortal(
  response: CanaryResponse,
  expectation: CanaryExpectation
): CaptivePortalClassification {
  if (response.status === null) {
    return { detected: false, method: 'none', redirectLocation: null, httpStatus: null };
  }
  if (response.status >= 300 && response.status < 400) {
    return {
      detected: true,
      method: 'redirect',
      redirectLocation: response.location,
      httpStatus: response.status,
    };
  }

  const statusMatches = response.status === expectation.expectedStatus;
  const bodyMatches =
    expectation.expectedBodyContains === undefined ||
    response.body.includes(expectation.expectedBodyContains);

  if (statusMatches && bodyMatches) {
    return { detected: false, method: 'none', redirectLocation: null, httpStatus: response.status };
  }
  return {
    detected: true,
    method: 'content-mismatch',
    redirectLocation: null,
    httpStatus: response.status,
  };
}

const CANARIES: { url: string; expectation: CanaryExpectation }[] = [
  { url: 'http://connectivitycheck.gstatic.com/generate_204', expectation: { expectedStatus: 204 } },
  {
    url: 'http://captive.apple.com/hotspot-detect.html',
    expectation: { expectedStatus: 200, expectedBodyContains: 'Success' },
  },
];

async function probeCaptivePortal(): Promise<CaptivePortalResult> {
  for (const canary of CANARIES) {
    try {
      const res = await axios.get(canary.url, {
        maxRedirects: 0,
        validateStatus: () => true,
        timeout: CAPTIVE_TIMEOUT_MS,
      });
      const location = res.headers['location'];
      const classification = classifyCaptivePortal(
        { status: res.status, location: typeof location === 'string' ? location : null, body: String(res.data ?? '') },
        canary.expectation
      );
      return { ...classification, canaryUrl: canary.url };
    } catch {
      continue;
    }
  }
  const first = CANARIES[0]!;
  return { detected: false, method: 'none', redirectLocation: null, canaryUrl: first.url, httpStatus: null };
}

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

function wifiFindings(encryption: WifiEncryption): Finding[] {
  switch (encryption) {
    case 'Open':
      return [{ id: 'security.wifi-open', severity: 'alert', points: 40, title: 'WiFi is open (unencrypted)' }];
    case 'WPA':
      return [{ id: 'security.wifi-wpa', severity: 'warn', points: 20, title: 'WiFi uses WPA, not WPA2/WPA3' }];
    case 'WPA2':
      return [{ id: 'security.wifi-wpa2', severity: 'info', points: 5, title: 'WiFi uses WPA2, not WPA3' }];
    case 'WPA3':
    case 'WPA2-Enterprise':
      return [{ id: 'security.wifi-strong', severity: 'good', points: 0, title: `WiFi uses ${encryption}` }];
    case 'Unknown':
      return [];
  }
}

function dnsLeakFindings(dnsLeak: DnsLeakResult): Finding[] {
  if (dnsLeak.verdict === 'leaked') {
    return [{ id: 'security.dns-leak', severity: 'alert', points: 25, title: 'DNS leak detected' }];
  }
  if (dnsLeak.verdict === 'uncertain') {
    return [
      {
        id: 'security.dns-leak-uncertain',
        severity: 'warn',
        points: 5,
        title: 'DNS leak status could not be verified',
      },
    ];
  }
  return [{ id: 'security.dns-clean', severity: 'good', points: 0, title: 'No DNS leak detected' }];
}

function captivePortalFindings(portal: CaptivePortalResult): Finding[] {
  if (portal.detected) {
    return [{ id: 'security.captive-portal', severity: 'warn', points: 15, title: 'Captive portal detected' }];
  }
  return [{ id: 'security.no-captive-portal', severity: 'good', points: 0, title: 'No captive portal detected' }];
}

// ---------------------------------------------------------------------------
// Check orchestration
// ---------------------------------------------------------------------------

export async function checkSecurity(
  iface: string | null,
  exec: ExecFn = execCmd
): Promise<CheckResult<SecurityData>> {
  const start = Date.now();

  const wifiResult = await exec([
    'nmcli',
    '-t',
    '-f',
    'active,ssid,security,chan,freq,signal',
    'dev',
    'wifi',
    'list',
  ]);
  const wifi = parseNmcliWifi(wifiResult.stdout);

  let dns: SecurityData['dns'] = null;
  if (iface) {
    const resolvectlResult = await exec(['resolvectl', 'status']);
    dns = parseResolvectlStatus(resolvectlResult.stdout, iface);
  }

  const [systemEgressIp, cloudflareProbe, googleProbe, captivePortal] = await Promise.all([
    getSystemEgressIp(exec),
    probeDoh('cloudflare'),
    probeDoh('google'),
    probeCaptivePortal(),
  ]);

  const dnsLeak = classifyDnsLeak(systemEgressIp, [cloudflareProbe, googleProbe]);
  const encryption = wifi?.encryption ?? 'Unknown';

  const data: SecurityData = {
    ssid: wifi?.ssid ?? null,
    encryption,
    channel: wifi?.channel ?? null,
    frequencyMhz: wifi?.frequencyMhz ?? null,
    signalPercent: wifi?.signalPercent ?? null,
    dns,
    dnsLeak,
    captivePortal,
  };

  const errors: string[] = [];
  if (iface && !dns) errors.push(`Could not determine DNS servers for ${iface}`);

  return {
    name: 'security',
    status: iface && !dns ? 'degraded' : 'ok',
    data,
    errors,
    findings: [...wifiFindings(encryption), ...dnsLeakFindings(dnsLeak), ...captivePortalFindings(captivePortal)],
    durationMs: Date.now() - start,
  };
}
