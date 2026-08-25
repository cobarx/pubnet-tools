import { spawn } from 'node:child_process';
import { Command } from 'commander';
import ora, { type Ora } from 'ora';
import { checkReliability } from './checks/reliability.js';
import { checkSecurity } from './checks/security.js';
import { checkSpeed } from './checks/speed.js';
import { checkTopology } from './checks/topology.js';
import { renderReport } from './output/renderer.js';
import { saveReport } from './output/reporter.js';
import { calculateScore } from './scoring.js';
import { execCmd } from './utils/exec.js';
import type { CheckStatus, Report } from './types.js';

const VERSION = '0.1.0';
const CHECK_NAMES = ['topology', 'security', 'reliability', 'speed'] as const;
type CheckName = (typeof CHECK_NAMES)[number];

function finishSpinner(spinner: Ora, status: CheckStatus, text: string): void {
  if (status === 'ok') spinner.succeed(text);
  else if (status === 'degraded' || status === 'skipped') spinner.warn(text);
  else spinner.fail(text);
}

/** Worst-of ordering so one shared spinner can reflect several checks' outcomes. */
function combinedStatus(statuses: CheckStatus[]): CheckStatus {
  if (statuses.includes('failed')) return 'failed';
  if (statuses.includes('degraded') || statuses.includes('skipped')) return 'degraded';
  return 'ok';
}

/**
 * A run passing cleanly is the common case — naming all four as "ok" every
 * time is noise, not information. Only the ones with something to report
 * get named; a clean run gets one terse line instead.
 */
function summarizeRunStatus(results: Array<{ label: string; status: CheckStatus }>): string {
  const issues = results.filter((r) => r.status !== 'ok');
  if (issues.length === 0) return 'All checks passed';
  return issues.map((r) => `${r.label}: ${r.status}`).join(' · ');
}

function excludedResult(name: string) {
  return {
    name,
    status: 'skipped' as const,
    data: null,
    errors: ['Excluded by --only'],
    findings: [],
    durationMs: 0,
  };
}

export interface RunAuditOptions {
  only?: CheckName[];
  quiet?: boolean;
}

export async function runAudit(options: RunAuditOptions = {}): Promise<Report> {
  const only = options.only;
  const shouldRun = (name: CheckName) => !only || only.includes(name);
  const willRunTopology = shouldRun('topology');
  const concurrentNames = (['security', 'reliability', 'speed'] as const).filter(shouldRun);

  // One spinner instance for the entire run — its text is updated between
  // phases rather than replaced, so there's never more than one live spinner
  // (ora warns on that: "Multiple concurrent spinners detected... visual
  // corruption") and the run ends on a single combined summary line.
  const initialLabel = willRunTopology
    ? 'Checking network topology...'
    : concurrentNames.length > 0
      ? 'Analyzing...'
      : null;
  const spinner = !options.quiet && initialLabel ? ora(initialLabel).start() : null;

  const topology = willRunTopology ? await checkTopology() : excludedResult('topology');

  const gatewayIp = topology.data?.gateway ?? null;
  const iface = topology.data?.interface ?? null;

  if (spinner && willRunTopology && concurrentNames.length > 0) {
    spinner.text = 'Analyzing...';
  }

  const [security, reliability, speed] = await Promise.all([
    shouldRun('security') ? checkSecurity(iface) : Promise.resolve(excludedResult('security')),
    shouldRun('reliability') ? checkReliability(gatewayIp) : Promise.resolve(excludedResult('reliability')),
    shouldRun('speed') ? checkSpeed() : Promise.resolve(excludedResult('speed')),
  ]);

  if (spinner) {
    const results: Array<{ label: string; status: CheckStatus }> = [];
    if (willRunTopology) results.push({ label: 'Topology', status: topology.status });
    if (shouldRun('security')) results.push({ label: 'Security', status: security.status });
    if (shouldRun('reliability')) results.push({ label: 'Reliability', status: reliability.status });
    if (shouldRun('speed')) results.push({ label: 'Speed', status: speed.status });

    finishSpinner(spinner, combinedStatus(results.map((r) => r.status)), summarizeRunStatus(results));
  }

  const score = calculateScore([topology, security, reliability, speed]);

  return {
    version: VERSION,
    timestamp: new Date().toISOString(),
    security: security as Report['security'],
    speed: speed as Report['speed'],
    reliability: reliability as Report['reliability'],
    topology: topology as Report['topology'],
    score,
  };
}

function parseOnly(value: string): CheckName[] {
  const requested = value.split(',').map((s) => s.trim());
  const valid = requested.filter((s): s is CheckName => (CHECK_NAMES as readonly string[]).includes(s));
  if (valid.length === 0) {
    throw new Error(`--only must be a comma-separated list from: ${CHECK_NAMES.join(', ')}`);
  }
  return valid;
}

async function runCommand(opts: { json?: boolean; save?: boolean; only?: string; strict?: boolean }): Promise<void> {
  const report = await runAudit({
    ...(opts.only ? { only: parseOnly(opts.only) } : {}),
    quiet: opts.json ?? false,
  });

  if (opts.json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(`${renderReport(report)}\n`);
  }

  if (opts.save) {
    const path = await saveReport(report);
    if (!opts.json) process.stdout.write(`Saved report to ${path}\n`);
  }

  if (opts.strict && (report.score.level === 'Medium' || report.score.level === 'High')) {
    process.exitCode = 1;
  }
}

async function detectAsciinemaVersion(): Promise<1 | 2 | 3 | null> {
  const which = await execCmd(['which', 'asciinema']);
  if (which.exitCode !== 0) return null;
  const versionResult = await execCmd(['asciinema', '--version']);
  const match = /(\d+)\.\d+\.\d+/.exec(versionResult.stdout);
  const major = match?.[1] ? Number(match[1]) : null;
  if (major === 2 || major === 3) return major;
  return 1;
}

async function recordCommand(): Promise<void> {
  const version = await detectAsciinemaVersion();
  if (version === null) {
    process.stderr.write('asciinema is not installed. Install it with: sudo pacman -S asciinema\n');
    process.exitCode = 1;
    return;
  }

  const timestamp = new Date().toISOString().replaceAll(':', '-').split('.')[0];
  const path = `${process.env['HOME']}/.conncheck/recordings/${timestamp}.cast`;
  await execCmd(['mkdir', '-p', `${process.env['HOME']}/.conncheck/recordings`]);

  const args =
    version >= 3
      ? ['rec', '--output', path, '--', 'conncheck']
      : ['rec', path, '--', 'conncheck'];

  await new Promise<void>((resolve) => {
    const child = spawn('asciinema', args, { stdio: 'inherit' });
    child.on('close', () => resolve());
  });
}

export function buildCli(): Command {
  const program = new Command();
  program.name('conncheck').description('Audit the public WiFi or network you just joined.').version(VERSION);

  program
    .option('--json', 'print JSON to stdout, suppress spinners')
    .option('--save', 'write the report to ~/.conncheck/reports/ (off by default)')
    .option('--only <checks>', 'comma list of checks to run: topology,security,reliability,speed')
    .option('--strict', 'exit non-zero on Medium/High risk')
    .action(runCommand);

  program
    .command('record')
    .description('wrap a full run in asciinema for session capture')
    .action(recordCommand);

  return program;
}
