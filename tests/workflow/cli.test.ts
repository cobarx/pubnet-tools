import { execFile } from 'node:child_process';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { afterEach, beforeEach, describe, expect, test } from 'vitest';

const execFileAsync = promisify(execFile);
const TSX = join(process.cwd(), 'node_modules', '.bin', 'tsx');
const ENTRY = join(process.cwd(), 'src', 'index.ts');

function runCli(args: string[], env: NodeJS.ProcessEnv = process.env) {
  return execFileAsync(TSX, [ENTRY, ...args], { env });
}

// Workflow test — via CLI. Real everything: real commands, real network,
// the real commander entry point. --only limits scope to the fast checks
// (topology/security/reliability); the individual checks' own correctness
// is covered by each check's contract tests — this proves the CLI wires
// them together correctly.
describe('conncheck CLI (workflow, via the real entry point)', () => {
  let home: string;
  beforeEach(async () => {
    home = await mkdtemp(join(tmpdir(), 'conncheck-home-'));
  });
  afterEach(async () => {
    await rm(home, { recursive: true, force: true });
  });

  test('--json --only=topology,security,reliability prints a full Report shape', async () => {
    const { stdout } = await runCli(['--json', '--only=topology,security,reliability']);
    const report = JSON.parse(stdout);

    expect(report.version).toBeTruthy();
    expect(typeof report.timestamp).toBe('string');
    expect(['Low', 'Medium', 'High']).toContain(report.score.level);
    expect(report.topology.status).not.toBe('failed');
    expect(report.speed.status).toBe('skipped');
    expect(report.speed.errors[0]).toContain('--only');
  }, 30_000);

  test('does not save a report file unless --save is passed', async () => {
    const { stdout } = await runCli(['--json', '--only=topology'], { ...process.env, HOME: home });
    JSON.parse(stdout);

    await expect(readdir(join(home, '.conncheck', 'reports'))).rejects.toMatchObject({ code: 'ENOENT' });
  }, 15_000);

  test('--save writes a report file under HOME/.conncheck/reports', async () => {
    const { stdout } = await runCli(['--json', '--only=topology', '--save'], { ...process.env, HOME: home });
    JSON.parse(stdout); // still valid JSON on stdout even though a file was also saved

    const files = await readdir(join(home, '.conncheck', 'reports'));
    expect(files.length).toBe(1);
    expect(files[0]).toMatch(/\.json$/);
  }, 15_000);

  test('--strict exits non-zero when risk is Medium or High', async () => {
    // topology-only on a real network is Low risk (no scored findings), so
    // assert the documented contract instead of forcing a specific risk
    // level: exit code matches whether the reported level is Medium/High.
    let exitCode = 0;
    let stdout = '';
    try {
      const result = await runCli(['--json', '--only=topology', '--strict']);
      stdout = result.stdout;
    } catch (err) {
      const e = err as { code?: number; stdout?: string };
      exitCode = e.code ?? 1;
      stdout = e.stdout ?? '';
    }
    const report = JSON.parse(stdout);
    const shouldFail = report.score.level === 'Medium' || report.score.level === 'High';
    expect(exitCode).toBe(shouldFail ? 1 : 0);
  }, 15_000);

  test('record subcommand fails gracefully when asciinema is not installed', async () => {
    // This dev machine genuinely has no asciinema installed (confirmed via
    // `which asciinema` returning nothing) — real absence, not simulated.
    await expect(runCli(['record'])).rejects.toMatchObject({
      code: 1,
      stderr: expect.stringContaining('asciinema is not installed'),
    });
  }, 15_000);
});
