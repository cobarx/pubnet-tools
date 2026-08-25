import { spawn } from 'node:child_process';

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
}

/**
 * Runs a command via spawn (array args, no shell — no injection surface).
 * Never rejects on non-zero exit; callers inspect exitCode. Only rejects
 * on actual spawn failure (e.g. ENOENT — the binary doesn't exist).
 */
export function execCmd(
  cmd: string[],
  timeoutMs = 10_000
): Promise<ExecResult> {
  return new Promise((resolve, reject) => {
    const [file, ...args] = cmd;
    if (!file) {
      reject(new Error('execCmd requires a non-empty command array'));
      return;
    }

    const child = spawn(file, args, {
      env: { ...process.env, LC_ALL: 'C' },
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;
    let settled = false;

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
    }, timeoutMs);

    child.stdout.on('data', (chunk: Buffer) => {
      stdout += chunk.toString('utf8');
    });
    child.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString('utf8');
    });

    child.on('error', (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(err);
    });

    child.on('close', (exitCode) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({ stdout, stderr, exitCode, timedOut });
    });
  });
}
