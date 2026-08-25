import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, test } from 'vitest';
import { saveReport } from '../../src/output/reporter.js';
import type { Report } from '../../src/types.js';

function fakeReport(): Report {
  const empty = { name: 'x', status: 'ok' as const, data: null, errors: [], findings: [], durationMs: 1 };
  return {
    version: '0.1.0',
    timestamp: '2026-08-24T12:34:56.789Z',
    security: empty,
    speed: empty,
    reliability: empty,
    topology: empty,
    score: { total: 0, level: 'Low', findings: [] },
  };
}

describe('saveReport', () => {
  let dir: string;
  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'conncheck-reports-'));
  });
  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  test('writes the report as JSON, named from its timestamp with colons replaced', async () => {
    const report = fakeReport();

    const path = await saveReport(report, dir);

    expect(path).toBe(join(dir, '2026-08-24T12-34-56.789Z.json'));
    const written = JSON.parse(await readFile(path, 'utf8'));
    expect(written).toEqual(report);
  });

  test('creates the reports directory if it does not exist yet', async () => {
    const nested = join(dir, 'nested', 'reports');
    const report = fakeReport();

    const path = await saveReport(report, nested);

    expect(await readFile(path, 'utf8')).toContain('"version"');
  });
});
