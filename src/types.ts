export type CheckStatus = 'ok' | 'degraded' | 'failed' | 'skipped';
// ok=complete, degraded=partial data, failed=no data, skipped=precondition absent

export type Severity = 'good' | 'warn' | 'alert' | 'info';

export interface Finding {
  id: string; // stable key e.g. 'wifi.open', 'dns.leak'
  severity: Severity;
  points: number; // 0 for good/info
  title: string;
  detail?: string;
}

export interface CheckResult<T> {
  name: string;
  status: CheckStatus;
  data: T | null; // null only when status === 'failed' | 'skipped'
  errors: string[];
  findings: Finding[];
  durationMs: number;
}

// --- Security ---

export type WifiEncryption = 'WPA3' | 'WPA2' | 'WPA2-Enterprise' | 'WPA' | 'Open' | 'Unknown';

export interface DnsResolverInfo {
  link: string;
  currentServer: string | null;
  servers: string[];
  source: 'resolvectl' | 'resolv.conf';
}

export interface DohProbe {
  provider: 'cloudflare' | 'google';
  egressIp: string | null;
  reachable: boolean;
}

export interface DnsLeakResult {
  systemEgressIp: string | null; // from resolvectl query whoami.cloudflare.com
  probes: DohProbe[];
  leaked: boolean;
  verdict: 'clean' | 'leaked' | 'uncertain'; // uncertain = all probes unreachable
}

export interface CaptivePortalResult {
  detected: boolean;
  method: 'redirect' | 'content-mismatch' | 'none';
  redirectLocation: string | null;
  canaryUrl: string;
  httpStatus: number | null;
}

export interface SecurityData {
  ssid: string | null;
  encryption: WifiEncryption;
  channel: number | null;
  frequencyMhz: number | null;
  signalPercent: number | null;
  dns: DnsResolverInfo | null;
  dnsLeak: DnsLeakResult;
  captivePortal: CaptivePortalResult;
}

// --- Speed ---

export interface SpeedData {
  downloadMbps: number;
  uploadMbps: number;
  latencyMs: number;
  jitterMs: number;
  source: 'ndt7';
}

// --- Reliability ---

export interface PingTargetResult {
  host: string;
  label: 'gateway' | 'google-dns' | 'cloudflare-dns';
  transmitted: number;
  received: number;
  packetLossPct: number;
  minMs: number | null;
  avgMs: number | null;
  maxMs: number | null;
  jitterMs: number | null; // stddev of individual RTTs (not ping's mdev)
  rtts: number[]; // per-packet RTTs from non-quiet output
  reachable: boolean;
}

export interface ReliabilityData {
  targets: PingTargetResult[];
  gatewayReachable: boolean;
  internetReachable: boolean;
}

// --- Topology ---

export interface ArpNeighbor {
  ip: string;
  mac: string | null;
  state: string;
  device: string;
  isGateway: boolean;
  vendor: string | null;
}

export interface TopologyData {
  interface: string;
  ipCidr: string; // e.g. "10.59.140.42/22"
  gateway: string;
  neighbors: ArpNeighbor[];
  passiveNotice: string; // always "Passive ARP cache — no active scan performed."
}

// --- Report ---

export type RiskLevel = 'Low' | 'Medium' | 'High';

export interface ScoreResult {
  total: number;
  level: RiskLevel;
  findings: Finding[];
}

export interface Report {
  version: string;
  timestamp: string; // ISO 8601
  security: CheckResult<SecurityData>;
  speed: CheckResult<SpeedData>;
  reliability: CheckResult<ReliabilityData>;
  topology: CheckResult<TopologyData>;
  score: ScoreResult;
}
