import { describe, expect, test } from 'vitest';
import {
  extractRemoteIp,
  ipFamily,
  isValidIPv4,
  lookupMacVendor,
  parseIpAddr,
  parseIpNeigh,
  parseIpRoute,
  parseNmcliWifi,
  parsePingOutput,
  parseResolvectlStatus,
  stddev,
} from '../../src/utils/network.js';

describe('parseNmcliWifi', () => {
  test('classifies the active row with WPA3 security and real channel/freq/signal fields', () => {
    const raw = [
      'no:Patels:WPA2 WPA3:8:2447 MHz:67',
      'no::WPA2:11:2462 MHz:70',
      'yes:ABVI_Dunnigan_Guest:WPA3:117:6535 MHz:50',
      'no:Super8_Admin:WPA2 WPA3:11:2462 MHz:69',
    ].join('\n');
    expect(parseNmcliWifi(raw)).toEqual({
      ssid: 'ABVI_Dunnigan_Guest',
      encryption: 'WPA3',
      channel: 117,
      frequencyMhz: 6535,
      signalPercent: 50,
    });
  });

  test('classifies a plain WPA2 active row', () => {
    const raw = 'yes:CorpNet:WPA2:6:2437 MHz:80';
    expect(parseNmcliWifi(raw)).toEqual({
      ssid: 'CorpNet',
      encryption: 'WPA2',
      channel: 6,
      frequencyMhz: 2437,
      signalPercent: 80,
    });
  });

  test('classifies a WPA2/WPA3 transition network by its strongest mode', () => {
    const raw = 'yes:Transition:WPA2 WPA3:6:2437 MHz:80';
    expect(parseNmcliWifi(raw)?.encryption).toBe('WPA3');
  });

  test('an empty security field means Open, per observed nmcli behavior', () => {
    const raw = 'yes:Berkeley-Visitor::6:2437 MHz:80';
    expect(parseNmcliWifi(raw)?.encryption).toBe('Open');
  });

  test('an 802.1X suffix means WPA2-Enterprise', () => {
    const raw = 'yes:CorpSecure:WPA2 802.1X:6:2437 MHz:80';
    expect(parseNmcliWifi(raw)?.encryption).toBe('WPA2-Enterprise');
  });

  test('an SSID containing a colon is unescaped, per nmcli terse-mode escaping', () => {
    // nmcli -t backslash-escapes ':' and '\' within field values.
    const raw = String.raw`yes:Cafe\: Downtown:WPA2:6:2437 MHz:80`;
    expect(parseNmcliWifi(raw)).toEqual({
      ssid: 'Cafe: Downtown',
      encryption: 'WPA2',
      channel: 6,
      frequencyMhz: 2437,
      signalPercent: 80,
    });
  });

  test('missing numeric fields are null rather than NaN', () => {
    const raw = 'yes:NoSignalData:WPA2';
    expect(parseNmcliWifi(raw)).toEqual({
      ssid: 'NoSignalData',
      encryption: 'WPA2',
      channel: null,
      frequencyMhz: null,
      signalPercent: null,
    });
  });

  test('no active row returns null', () => {
    const raw = ['no:Patels:WPA2 WPA3', 'no::WPA2'].join('\n');
    expect(parseNmcliWifi(raw)).toBeNull();
  });
});

describe('parseIpRoute', () => {
  test('extracts gateway and device from a real default route line', () => {
    const raw =
      'default via 192.168.5.1 dev wlan0 proto dhcp src 192.168.5.151 metric 600 \n';
    expect(parseIpRoute(raw)).toEqual({ gateway: '192.168.5.1', device: 'wlan0' });
  });

  test('no default route returns null', () => {
    expect(parseIpRoute('')).toBeNull();
  });
});

describe('parseIpAddr', () => {
  test('extracts the IPv4 address and prefix, ignoring inet6', () => {
    const raw = `2: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000
    link/ether 9c:67:d6:bb:58:2d brd ff:ff:ff:ff:ff:ff
    inet 192.168.5.151/24 brd 192.168.5.255 scope global dynamic noprefixroute wlan0
       valid_lft 6695sec preferred_lft 6695sec
    inet6 fe80::f3a:aa70:6a12:23a8/64 scope link noprefixroute
       valid_lft forever preferred_lft forever`;
    expect(parseIpAddr(raw)).toEqual({ ip: '192.168.5.151', prefix: 24 });
  });

  test('no inet line returns null', () => {
    expect(parseIpAddr('2: wlan0: <BROADCAST> mtu 1500')).toBeNull();
  });
});

describe('parseIpNeigh', () => {
  test('parses neighbors and flags the gateway', () => {
    const raw = [
      '192.168.5.1 lladdr 68:7f:f0:55:77:7b REACHABLE ',
      '192.168.5.60 lladdr 68:72:c3:87:16:66 STALE ',
    ].join('\n');
    expect(parseIpNeigh(raw, 'wlan0', '192.168.5.1')).toEqual([
      {
        ip: '192.168.5.1',
        mac: '68:7f:f0:55:77:7b',
        state: 'REACHABLE',
        device: 'wlan0',
        isGateway: true,
        vendor: 'TP-Link',
      },
      {
        // Samsung — a real device vendor, but not in the curated
        // networking-equipment table, so this exercises the "known MAC,
        // unrecognized/non-networking vendor" case: null, not a guess.
        ip: '192.168.5.60',
        mac: '68:72:c3:87:16:66',
        state: 'STALE',
        device: 'wlan0',
        isGateway: false,
        vendor: null,
      },
    ]);
  });

  test('an incomplete entry with no lladdr has a null mac and vendor', () => {
    const raw = '192.168.5.99 INCOMPLETE ';
    expect(parseIpNeigh(raw, 'wlan0', '192.168.5.1')).toEqual([
      {
        ip: '192.168.5.99',
        mac: null,
        state: 'INCOMPLETE',
        device: 'wlan0',
        isGateway: false,
        vendor: null,
      },
    ]);
  });

  test('an empty ARP cache returns an empty list', () => {
    expect(parseIpNeigh('', 'wlan0', '192.168.5.1')).toEqual([]);
  });
});

describe('parsePingOutput', () => {
  test('parses per-packet RTTs and the transmit/receive summary', () => {
    const raw = `PING 1.1.1.1 (1.1.1.1) 56(84) bytes of data.
64 bytes from 1.1.1.1: icmp_seq=1 ttl=56 time=15.4 ms
64 bytes from 1.1.1.1: icmp_seq=2 ttl=56 time=9.69 ms
64 bytes from 1.1.1.1: icmp_seq=3 ttl=56 time=20.9 ms

--- 1.1.1.1 ping statistics ---
3 packets transmitted, 3 received, 0% packet loss, time 401ms
rtt min/avg/max/mdev = 9.685/15.348/20.918/4.586 ms`;
    expect(parsePingOutput(raw)).toEqual({
      transmitted: 3,
      received: 3,
      rtts: [15.4, 9.69, 20.9],
    });
  });

  test('100% packet loss reports zero received and no RTTs', () => {
    const raw = `PING 10.0.0.99 (10.0.0.99) 56(84) bytes of data.

--- 10.0.0.99 ping statistics ---
10 packets transmitted, 0 received, 100% packet loss, time 2049ms`;
    expect(parsePingOutput(raw)).toEqual({ transmitted: 10, received: 0, rtts: [] });
  });
});

describe('parseResolvectlStatus', () => {
  const raw = `Global
           Protocols: +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported
    resolv.conf mode: foreign
Fallback DNS Servers: 9.9.9.9#dns.quad9.net 2620:fe::9#dns.quad9.net
                      1.1.1.1#cloudflare-dns.com

Link 2 (wlan0)
    Current Scopes: DNS LLMNR/IPv4 LLMNR/IPv6 mDNS/IPv4 mDNS/IPv6
         Protocols: +DefaultRoute +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported
Current DNS Server: 192.168.5.1
       DNS Servers: 192.168.5.1
     Default Route: yes

Link 4 (vmnet8)
    Current Scopes: LLMNR/IPv4 LLMNR/IPv6 mDNS/IPv4 mDNS/IPv6
         Protocols: -DefaultRoute +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported
     Default Route: no`;

  test('parses the active link block, ignoring the global Fallback DNS Servers', () => {
    expect(parseResolvectlStatus(raw, 'wlan0')).toEqual({
      link: 'wlan0',
      currentServer: '192.168.5.1',
      servers: ['192.168.5.1'],
      source: 'resolvectl',
    });
  });

  test('a link with no matching block returns null', () => {
    expect(parseResolvectlStatus(raw, 'eth9')).toBeNull();
  });
});

describe('stddev', () => {
  test('population standard deviation of a known set', () => {
    expect(stddev([2, 4, 4, 4, 5, 5, 7, 9])).toBeCloseTo(2, 5);
  });

  test('a single value has zero deviation', () => {
    expect(stddev([42])).toBe(0);
  });

  test('an empty array has zero deviation', () => {
    expect(stddev([])).toBe(0);
  });
});

describe('lookupMacVendor', () => {
  test.each([
    // Prefixes verified against the real IEEE OUI registry (standards-oui.ieee.org),
    // not invented — includes this dev machine's own real gateway MAC.
    ['68:7f:f0:55:77:7b', 'TP-Link'], // real gateway MAC seen on this dev machine
    ['34:f7:16:aa:bb:cc', 'TP-Link'],
    ['40:5d:82:aa:bb:cc', 'NETGEAR'],
    ['00:26:18:aa:bb:cc', 'ASUSTek'],
    ['bc:22:28:aa:bb:cc', 'D-Link'],
    ['f0:9f:c2:aa:bb:cc', 'Ubiquiti'],
    ['e8:0a:b9:aa:bb:cc', 'Cisco Systems'],
    ['f0:ee:7a:aa:bb:cc', 'Apple'],
    ['08:55:31:aa:bb:cc', 'MikroTik'],
  ])('%s -> %s', (mac, vendor) => {
    expect(lookupMacVendor(mac)).toBe(vendor);
  });

  test('an unrecognized prefix returns null', () => {
    expect(lookupMacVendor('02:00:00:aa:bb:cc')).toBeNull();
  });

  test('a null MAC returns null', () => {
    expect(lookupMacVendor(null)).toBeNull();
  });

  test('is case-insensitive and separator-insensitive', () => {
    expect(lookupMacVendor('68-7F-F0-55-77-7B')).toBe('TP-Link');
  });
});

describe('extractRemoteIp', () => {
  test('extracts an IPv6 remote_ip from real resolvectl TXT query output', () => {
    const raw = `whoami.cloudflare.com IN TXT "ecs_subnet: 12.156.9.0/24"    -- link: wlan0
whoami.cloudflare.com IN TXT "country_code: US"             -- link: wlan0
whoami.cloudflare.com IN TXT "remote_ip: 2607:f8b0:4004:1001::12e" -- link: wlan0
whoami.cloudflare.com IN TXT "asn: 7018"                    -- link: wlan0`;
    expect(extractRemoteIp(raw)).toBe('2607:f8b0:4004:1001::12e');
  });

  test('extracts an IPv4 remote_ip from Cloudflare DoH JSON, despite its escaped inner quotes', () => {
    const raw =
      '{"Answer":[{"data":"\\"asn: 13335\\""},{"data":"\\"remote_ip: 162.159.0.0\\""}]}';
    expect(extractRemoteIp(raw)).toBe('162.159.0.0');
  });

  test('extracts an IPv6 remote_ip from Google DoH JSON, which has no inner quoting', () => {
    const raw = '{"Answer":[{"data":"remote_ip: 2607:f8b0:4004:1009::12c"}]}';
    expect(extractRemoteIp(raw)).toBe('2607:f8b0:4004:1009::12c');
  });

  test('no remote_ip field returns null', () => {
    expect(extractRemoteIp('{"Answer":[{"data":"asn: 13335"}]}')).toBeNull();
  });
});

describe('ipFamily', () => {
  test.each([
    ['1.1.1.1', 'v4'],
    ['192.168.5.151', 'v4'],
    ['2607:f8b0:4004:1001::12e', 'v6'],
    ['::1', 'v6'],
  ] as const)('%s -> %s', (ip, family) => {
    expect(ipFamily(ip)).toBe(family);
  });
});

describe('isValidIPv4', () => {
  test.each([
    ['1.1.1.1', true],
    ['192.168.5.151', true],
    ['255.255.255.255', true],
    ['0.0.0.0', true],
    ['256.1.1.1', false],
    ['1.1.1', false],
    ['1.1.1.1.1', false],
    ['not-an-ip', false],
    ['', false],
  ])('%s -> %s', (input, expected) => {
    expect(isValidIPv4(input)).toBe(expected);
  });
});
