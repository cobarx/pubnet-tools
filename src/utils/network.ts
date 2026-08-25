import type { ArpNeighbor, DnsResolverInfo, WifiEncryption } from '../types.js';

export interface NmcliWifiResult {
  ssid: string;
  encryption: WifiEncryption;
  channel: number | null;
  frequencyMhz: number | null;
  signalPercent: number | null;
}

function classifySecurity(security: string): WifiEncryption {
  if (security === '') return 'Open';
  if (security.includes('802.1X')) return 'WPA2-Enterprise';
  if (security.includes('WPA3')) return 'WPA3';
  if (security.includes('WPA2')) return 'WPA2';
  if (security.includes('WPA')) return 'WPA';
  return 'Unknown';
}

/**
 * nmcli's terse (`-t`) output backslash-escapes ':' and '\' within field
 * values, so a colon in an SSID doesn't collide with the field delimiter.
 * Splits on unescaped colons only, then unescapes each field.
 */
function splitTerseFields(line: string): string[] {
  const fields: string[] = [];
  let current = '';
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '\\' && i + 1 < line.length) {
      current += line[i + 1];
      i++;
    } else if (ch === ':') {
      fields.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  fields.push(current);
  return fields;
}

function parseIntOrNull(value: string | undefined): number | null {
  if (value === undefined || value === '') return null;
  const n = Number.parseInt(value, 10);
  return Number.isNaN(n) ? null : n;
}

export function parseNmcliWifi(raw: string): NmcliWifiResult | null {
  for (const line of raw.split('\n')) {
    const [active, ssid, security, chan, freq, signal] = splitTerseFields(line);
    if (active !== 'yes' || ssid === undefined || security === undefined) continue;
    return {
      ssid,
      encryption: classifySecurity(security),
      channel: parseIntOrNull(chan),
      frequencyMhz: parseIntOrNull(freq),
      signalPercent: parseIntOrNull(signal),
    };
  }
  return null;
}

export interface IpRouteResult {
  gateway: string;
  device: string;
}

export function parseIpRoute(raw: string): IpRouteResult | null {
  const match = /^default via (\S+) dev (\S+)/m.exec(raw);
  if (!match) return null;
  const [, gateway, device] = match;
  if (!gateway || !device) return null;
  return { gateway, device };
}

export interface IpAddrResult {
  ip: string;
  prefix: number;
}

export function parseIpAddr(raw: string): IpAddrResult | null {
  const match = /^\s*inet (\d+\.\d+\.\d+\.\d+)\/(\d+)/m.exec(raw);
  if (!match) return null;
  const [, ip, prefixStr] = match;
  if (!ip || !prefixStr) return null;
  return { ip, prefix: Number(prefixStr) };
}

/**
 * OUI (first 3 octets of a MAC) to vendor name, keyed by 6-hex-digit
 * uppercase prefix. A curated subset of consumer/SOHO networking and
 * smart-home equipment vendors — not the full ~30k-entry IEEE registry,
 * which is mostly irrelevant hardware (automotive, medical, industrial)
 * that would never turn up as a home gateway or ARP neighbor. Every
 * prefix here is verified against the real IEEE OUI registry
 * (standards-oui.ieee.org/oui/oui.txt), not invented.
 */
const OUI_VENDORS: Record<string, string> = {
  '687FF0': 'TP-Link',
  '34F716': 'TP-Link',
  '54A703': 'TP-Link',
  'B0BE76': 'TP-Link',
  '405D82': 'NETGEAR',
  'DCEF09': 'NETGEAR',
  '100C6B': 'NETGEAR',
  '002618': 'ASUSTek',
  '049226': 'ASUSTek',
  '1831BF': 'ASUSTek',
  'BC2228': 'D-Link',
  'A0A3F0': 'D-Link',
  'BC0F9A': 'D-Link',
  '001D7E': 'Cisco-Linksys',
  '0014BF': 'Cisco-Linksys',
  '48F8B3': 'Cisco-Linksys',
  'D8EC5E': 'Belkin',
  'E89F80': 'Belkin',
  '58EF68': 'Belkin',
  'F09FC2': 'Ubiquiti',
  '802AA8': 'Ubiquiti',
  '788A20': 'Ubiquiti',
  'E80AB9': 'Cisco Systems',
  '481BA4': 'Cisco Systems',
  '6C03B5': 'Cisco Systems',
  '0015D1': 'CommScope',
  '2C301A': 'Technicolor',
  'FC2BB2': 'Actiontec',
  'A0A3E2': 'Actiontec',
  '5016F4': 'Motorola Mobility',
  'C4A052': 'Motorola Mobility',
  '6070C6': 'Google',
  'C82ADD': 'Google',
  '242934': 'Google',
  '842859': 'Amazon',
  '2873F6': 'Amazon',
  'E0CB1D': 'Amazon',
  'F0EE7A': 'Apple',
  '58AD12': 'Apple',
  '60FDA6': 'Apple',
  'E00630': 'Huawei',
  'D8DAF1': 'Huawei',
  '581DD8': 'Sagemcom',
  'C03C04': 'Sagemcom',
  'F80DA9': 'Zyxel',
  '88ACC0': 'Zyxel',
  '08F01E': 'eero',
  '98ED7E': 'eero',
  '80DA13': 'eero',
  '00043C': 'Sonos',
  '7828CA': 'Sonos',
  '085531': 'MikroTik',
  'B869F4': 'MikroTik',
  '000C42': 'MikroTik',
};

export function lookupMacVendor(mac: string | null): string | null {
  if (mac === null) return null;
  const prefix = mac.replaceAll(/[:-]/g, '').toUpperCase().slice(0, 6);
  return OUI_VENDORS[prefix] ?? null;
}

export function parseIpNeigh(raw: string, device: string, gatewayIp: string | null): ArpNeighbor[] {
  const neighbors: ArpNeighbor[] = [];
  for (const line of raw.split('\n')) {
    const trimmed = line.trim();
    if (trimmed === '') continue;
    const parts = trimmed.split(/\s+/);
    const ip = parts[0];
    if (!ip) continue;
    const lladdrIndex = parts.indexOf('lladdr');
    const mac = lladdrIndex !== -1 ? (parts[lladdrIndex + 1] ?? null) : null;
    const state = parts[parts.length - 1] ?? 'UNKNOWN';
    neighbors.push({
      ip,
      mac,
      state,
      device,
      isGateway: gatewayIp !== null && ip === gatewayIp,
      vendor: lookupMacVendor(mac),
    });
  }
  return neighbors;
}

export interface PingSummary {
  transmitted: number;
  received: number;
  rtts: number[];
}

export function parsePingOutput(raw: string): PingSummary {
  const rtts: number[] = [];
  for (const match of raw.matchAll(/time=([\d.]+) ms/g)) {
    const value = match[1];
    if (value !== undefined) rtts.push(Number(value));
  }

  const summaryMatch = /(\d+) packets transmitted, (\d+) received/.exec(raw);
  const transmitted = summaryMatch?.[1] !== undefined ? Number(summaryMatch[1]) : 0;
  const received = summaryMatch?.[2] !== undefined ? Number(summaryMatch[2]) : 0;

  return { transmitted, received, rtts };
}

export function parseResolvectlStatus(raw: string, iface: string): DnsResolverInfo | null {
  const linkHeaderPattern = new RegExp(`^Link \\d+ \\(${iface}\\)$`, 'm');
  const headerMatch = linkHeaderPattern.exec(raw);
  if (!headerMatch) return null;

  const blockStart = headerMatch.index + headerMatch[0].length;
  const nextHeaderMatch = /^Link \d+ \(/m.exec(raw.slice(blockStart));
  const blockEnd = nextHeaderMatch ? blockStart + nextHeaderMatch.index : raw.length;
  const block = raw.slice(blockStart, blockEnd);

  const currentServerMatch = /^Current DNS Server: (\S+)/m.exec(block);
  const serversMatch = /^\s*DNS Servers: (.+)$/m.exec(block);

  const currentServer = currentServerMatch?.[1] ?? null;
  const servers = serversMatch?.[1] ? serversMatch[1].trim().split(/\s+/) : [];

  return { link: iface, currentServer, servers, source: 'resolvectl' };
}

export function stddev(values: number[]): number {
  if (values.length === 0) return 0;
  const mean = values.reduce((sum, v) => sum + v, 0) / values.length;
  const variance = values.reduce((sum, v) => sum + (v - mean) ** 2, 0) / values.length;
  return Math.sqrt(variance);
}

const IPV4_PATTERN = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

export function isValidIPv4(s: string): boolean {
  const match = IPV4_PATTERN.exec(s);
  if (!match) return false;
  return match.slice(1).every((octet) => Number(octet) <= 255);
}

/**
 * Extracts a `remote_ip` value from raw response text — tolerant of
 * resolvectl's quoted TXT line, Google DoH's unquoted `data` field, and
 * Cloudflare DoH's escaped-quote `data` field, without needing to parse
 * JSON: the surrounding punctuation just isn't part of the character class.
 */
export function extractRemoteIp(raw: string): string | null {
  const match = /remote_ip:\s*([0-9a-fA-F.:]+)/.exec(raw);
  return match?.[1] ?? null;
}

export function ipFamily(ip: string): 'v4' | 'v6' {
  return isValidIPv4(ip) ? 'v4' : 'v6';
}
