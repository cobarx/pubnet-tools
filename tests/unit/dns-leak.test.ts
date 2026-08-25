import { describe, expect, test } from 'vitest';
import { classifyDnsLeak } from '../../src/checks/security.js';

describe('classifyDnsLeak', () => {
  // spec: dns-leak-detection#S1
  test('two comparable, agreeing probes produce clean', () => {
    const result = classifyDnsLeak('203.0.113.9', [
      { provider: 'cloudflare', reachable: true, egressIp: '203.0.113.4' },
      { provider: 'google', reachable: true, egressIp: '203.0.113.200' },
    ]);
    expect(result.verdict).toBe('clean');
    expect(result.leaked).toBe(false);
  });

  // spec: dns-leak-detection#S2
  test('every probe unreachable produces uncertain, never clean', () => {
    const result = classifyDnsLeak('203.0.113.9', [
      { provider: 'cloudflare', reachable: false, egressIp: null },
      { provider: 'google', reachable: false, egressIp: null },
    ]);
    expect(result.verdict).toBe('uncertain');
    expect(result.leaked).toBe(false);
  });

  // spec: dns-leak-detection#S3
  test('a comparable, disagreeing probe produces leaked', () => {
    const result = classifyDnsLeak('203.0.113.9', [
      { provider: 'cloudflare', reachable: true, egressIp: '198.51.100.4' },
      { provider: 'google', reachable: true, egressIp: '203.0.113.200' },
    ]);
    expect(result.verdict).toBe('leaked');
    expect(result.leaked).toBe(true);
    const cf = result.probes.find((p) => p.provider === 'cloudflare')!;
    expect(cf.egressIp).toBe('198.51.100.4');
  });

  // spec: dns-leak-detection#S4
  test('one reachable agreeing probe is enough for clean; the other is marked unreachable', () => {
    const result = classifyDnsLeak('203.0.113.9', [
      { provider: 'cloudflare', reachable: true, egressIp: '203.0.113.4' },
      { provider: 'google', reachable: false, egressIp: null },
    ]);
    expect(result.verdict).toBe('clean');
    const google = result.probes.find((p) => p.provider === 'google')!;
    expect(google.reachable).toBe(false);
  });

  // spec: dns-leak-detection#S5
  test('a family-mismatched probe (IPv6 vs system IPv4) counts as neither agree nor disagree', () => {
    const result = classifyDnsLeak('203.0.113.9', [
      { provider: 'cloudflare', reachable: true, egressIp: '2607:f8b0:4004:1001::12e' },
      { provider: 'google', reachable: true, egressIp: '203.0.113.4' },
    ]);
    expect(result.verdict).toBe('clean');
  });

  // spec: dns-leak-detection#S5
  test('a family-mismatched probe with no other comparable probe is uncertain', () => {
    const result = classifyDnsLeak('2607:f8b0:4004:1001::12e', [
      { provider: 'cloudflare', reachable: true, egressIp: '203.0.113.4' },
      { provider: 'google', reachable: true, egressIp: '2607:f8b0:4004:1009::12c' },
    ]);
    expect(result.verdict).toBe('uncertain');
  });

  test('no system egress IP is uncertain', () => {
    const result = classifyDnsLeak(null, [
      { provider: 'cloudflare', reachable: true, egressIp: '203.0.113.4' },
    ]);
    expect(result.verdict).toBe('uncertain');
  });
});
