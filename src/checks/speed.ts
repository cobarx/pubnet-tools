import { randomBytes } from 'node:crypto';
import WebSocket from 'ws';
import { stddev } from '../utils/network.js';
import type { CheckResult, Finding, SpeedData } from '../types.js';

/**
 * NDT7 (M-Lab) client, implemented directly against the protocol rather
 * than the official @m-lab/ndt7 package — that package assumes a browser
 * (Web Workers). See
 * docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md
 * for why @cloudflare/speedtest (the original choice) doesn't work at all
 * in Node.
 */
const NDT7_SUBPROTOCOL = 'net.measurementlab.ndt.v7';
const LOCATE_URL = 'https://locate.measurementlab.net/v2/nearest/ndt/ndt7';
const TEST_DURATION_MS = 10_000;
const UPLOAD_CHUNK_BYTES = 8192;
const MAX_BUFFERED_BYTES = UPLOAD_CHUNK_BYTES * 8;

export interface NdtServer {
  downloadUrl: string;
  uploadUrl: string;
}

export type LocateFn = () => Promise<NdtServer>;

interface LocateResponse {
  results?: { urls?: Record<string, string> }[];
}

export async function defaultLocate(): Promise<NdtServer> {
  const res = await fetch(LOCATE_URL);
  if (!res.ok) throw new Error(`NDT7 locate API returned HTTP ${res.status}`);
  const data = (await res.json()) as LocateResponse;
  const first = data.results?.[0];
  const downloadUrl = first?.urls?.['wss:///ndt/v7/download'];
  const uploadUrl = first?.urls?.['wss:///ndt/v7/upload'];
  if (!downloadUrl || !uploadUrl) throw new Error('NDT7 locate API returned no usable server');
  return { downloadUrl, uploadUrl };
}

/** The subset of `ws`'s WebSocket surface this client actually uses. */
export interface SocketLike {
  bufferedAmount: number;
  send(data: Buffer): void;
  close(): void;
  on(event: 'open', listener: () => void): unknown;
  on(event: 'message', listener: (data: Buffer, isBinary: boolean) => void): unknown;
  on(event: 'close', listener: () => void): unknown;
  on(event: 'error', listener: (err: Error) => void): unknown;
}

export type WebSocketFactory = (url: string) => SocketLike;

export function defaultWebSocketFactory(url: string): SocketLike {
  return new WebSocket(url, NDT7_SUBPROTOCOL) as unknown as SocketLike;
}

export function extractRttMs(json: string): number | null {
  try {
    const parsed = JSON.parse(json) as { TCPInfo?: { RTT?: number } };
    const rtt = parsed.TCPInfo?.RTT;
    return typeof rtt === 'number' ? rtt / 1000 : null;
  } catch {
    return null;
  }
}

export function mbps(bytes: number, elapsedMs: number): number {
  if (elapsedMs <= 0) return 0;
  return (bytes * 8) / (elapsedMs / 1000) / 1_000_000;
}

interface DirectionResult {
  bytesTransferred: number;
  elapsedMs: number;
  rttSamplesMs: number[];
}

function measureDirection(
  createSocket: WebSocketFactory,
  url: string,
  mode: 'download' | 'upload'
): Promise<DirectionResult> {
  return new Promise((resolve, reject) => {
    const socket = createSocket(url);
    let bytes = 0;
    const rtts: number[] = [];
    let startedAt: number | null = null;
    let settled = false;
    let sending = false;

    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      sending = false;
      clearTimeout(durationTimer);
      try {
        socket.close();
      } catch {
        // already closing/closed
      }
      if (err) reject(err);
      else resolve({ bytesTransferred: bytes, elapsedMs: Date.now() - (startedAt ?? Date.now()), rttSamplesMs: rtts });
    };

    const durationTimer = setTimeout(() => finish(), TEST_DURATION_MS);

    const chunk = mode === 'upload' ? randomBytes(UPLOAD_CHUNK_BYTES) : null;
    function sendLoop() {
      if (!sending || !chunk) return;
      if (socket.bufferedAmount < MAX_BUFFERED_BYTES) {
        socket.send(chunk);
        bytes += chunk.length;
      }
      setImmediate(sendLoop);
    }

    socket.on('open', () => {
      startedAt = Date.now();
      if (mode === 'upload') {
        sending = true;
        sendLoop();
      }
    });
    socket.on('message', (data: Buffer, isBinary: boolean) => {
      if (isBinary) {
        if (mode === 'download') bytes += data.length;
      } else {
        const rtt = extractRttMs(data.toString());
        if (rtt !== null) rtts.push(rtt);
      }
    });
    socket.on('error', (err) => finish(err instanceof Error ? err : new Error(String(err))));
    socket.on('close', () => finish());
  });
}

function failed(start: number, message: string): CheckResult<SpeedData> {
  const findings: Finding[] = [{ id: 'speed.failed', severity: 'warn', points: 5, title: 'Speed check failed' }];
  return {
    name: 'speed',
    status: 'failed',
    data: null,
    errors: [message],
    findings,
    durationMs: Date.now() - start,
  };
}

export async function checkSpeed(
  locate: LocateFn = defaultLocate,
  createSocket: WebSocketFactory = defaultWebSocketFactory
): Promise<CheckResult<SpeedData>> {
  const start = Date.now();

  let server: NdtServer;
  try {
    server = await locate();
  } catch (err) {
    return failed(start, err instanceof Error ? err.message : String(err));
  }

  let download: DirectionResult;
  let upload: DirectionResult;
  try {
    download = await measureDirection(createSocket, server.downloadUrl, 'download');
    upload = await measureDirection(createSocket, server.uploadUrl, 'upload');
  } catch (err) {
    return failed(start, err instanceof Error ? err.message : String(err));
  }

  const rtts = [...download.rttSamplesMs, ...upload.rttSamplesMs];
  const data: SpeedData = {
    downloadMbps: mbps(download.bytesTransferred, download.elapsedMs),
    uploadMbps: mbps(upload.bytesTransferred, upload.elapsedMs),
    latencyMs: rtts.length > 0 ? Math.min(...rtts) : 0,
    jitterMs: rtts.length > 0 ? stddev(rtts) : 0,
    source: 'ndt7',
  };

  const findings: Finding[] =
    data.downloadMbps < 1
      ? [{ id: 'speed.slow-download', severity: 'warn', points: 10, title: 'Download speed below 1 Mbps' }]
      : [];

  return {
    name: 'speed',
    status: 'ok',
    data,
    errors: [],
    findings,
    durationMs: Date.now() - start,
  };
}
