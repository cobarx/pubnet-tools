import { mkdir, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { Report } from '../types.js';

const DEFAULT_REPORTS_DIR = join(homedir(), '.conncheck', 'reports');

export async function saveReport(report: Report, reportsDir: string = DEFAULT_REPORTS_DIR): Promise<string> {
  await mkdir(reportsDir, { recursive: true });
  const filename = `${report.timestamp.replaceAll(':', '-')}.json`;
  const path = join(reportsDir, filename);
  await writeFile(path, JSON.stringify(report, null, 2), 'utf8');
  return path;
}
