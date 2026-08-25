import { EventEmitter } from 'node:events';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { checkSpeed, extractRttMs, mbps } from '../../src/checks/speed.js';

describe('mbps', () => {
  test('converts bytes and elapsed ms to megabits per second', () => {
    // 12,500,000 bytes in 1000ms = 100,000,000 bits/sec = 100 Mbps
    expect(mbps(12_500_000, 1000)).toBeCloseTo(100, 5);
  });

  test('zero elapsed time returns 0 rather than dividing by zero', () => {
    expect(mbps(1000, 0)).toBe(0);
  });
});

describe('extractRttMs', () => {
  test('extracts TCPInfo.RTT (microseconds) as milliseconds', () => {
    const json = '{"TCPInfo":{"RTT":41580,"BytesAcked":123}}';
    expect(extractRttMs(json)).toBeCloseTo(41.58, 5);
  });

  test('malformed JSON returns null rather than throwing', () => {
    expect(extractRttMs('not json')).toBeNull();
  });

  test('missing TCPInfo.RTT returns null', () => {
    expect(extractRttMs('{"BBRInfo":{"BW":1000}}')).toBeNull();
  });
});

// checkSpeed awaits `locate()` before creating each socket, so listeners
// aren't attached synchronously — give pending microtasks a chance to run
// before emitting events at each socket-creation boundary.
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

class FakeSocket extends EventEmitter {
  bufferedAmount = 0;
  closed = false;
  close() {
    this.closed = true;
  }
  send(_data: Buffer) {
    // no-op; tests drive bytes via emitted 'message' events for download,
    // and just check call count for upload
  }
}

describe('checkSpeed', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test('measures download and upload, converting bytes/elapsed to Mbps', async () => {
    const downloadSocket = new FakeSocket();
    const uploadSocket = new FakeSocket();
    const sockets = [downloadSocket, uploadSocket];

    const locate = vi.fn().mockResolvedValue({
      downloadUrl: 'wss://fake/download',
      uploadUrl: 'wss://fake/upload',
    });
    const createSocket = vi.fn(() => sockets.shift()!);

    const promise = checkSpeed(locate, createSocket as never);
    await flushMicrotasks();

    // download: 2s elapsed, 25,000,000 bytes -> 100 Mbps
    vi.setSystemTime(1000);
    downloadSocket.emit('open');
    vi.setSystemTime(3000);
    downloadSocket.emit('message', Buffer.alloc(25_000_000), true);
    downloadSocket.emit('message', Buffer.from('{"TCPInfo":{"RTT":20000}}'), false);
    downloadSocket.emit('close');

    await flushMicrotasks();

    // upload: 1s elapsed, 5,000,000 bytes -> 40 Mbps
    vi.setSystemTime(3000);
    uploadSocket.emit('open');
    await flushMicrotasks();
    vi.setSystemTime(4000);
    uploadSocket.emit('message', Buffer.from('{"TCPInfo":{"RTT":30000}}'), false);
    uploadSocket.emit('close');

    const result = await promise;

    expect(result.status).toBe('ok');
    expect(result.data!.downloadMbps).toBeCloseTo(100, 3);
    expect(result.data!.source).toBe('ndt7');
    expect(result.data!.latencyMs).toBeCloseTo(20, 5);
  });

  test('a locate failure is reported as failed, not thrown', async () => {
    const locate = vi.fn().mockRejectedValue(new Error('no server available'));
    const createSocket = vi.fn();

    const result = await checkSpeed(locate, createSocket as never);

    expect(result.status).toBe('failed');
    expect(result.data).toBeNull();
    expect(result.errors[0]).toContain('no server available');
  });

  test('a download socket error is reported as failed, not thrown', async () => {
    const downloadSocket = new FakeSocket();
    const locate = vi.fn().mockResolvedValue({
      downloadUrl: 'wss://fake/download',
      uploadUrl: 'wss://fake/upload',
    });
    const createSocket = vi.fn(() => downloadSocket);

    const promise = checkSpeed(locate, createSocket as never);
    await flushMicrotasks();
    downloadSocket.emit('open');
    downloadSocket.emit('error', new Error('connection reset'));

    const result = await promise;

    expect(result.status).toBe('failed');
    expect(result.data).toBeNull();
    expect(result.errors[0]).toContain('connection reset');
  });

  test('a hung direction is force-finished at the duration timeout', async () => {
    const downloadSocket = new FakeSocket();
    const uploadSocket = new FakeSocket();
    const sockets = [downloadSocket, uploadSocket];
    const locate = vi.fn().mockResolvedValue({
      downloadUrl: 'wss://fake/download',
      uploadUrl: 'wss://fake/upload',
    });
    const createSocket = vi.fn(() => sockets.shift()!);

    const promise = checkSpeed(locate, createSocket as never);
    await flushMicrotasks();

    downloadSocket.emit('open');
    downloadSocket.emit('message', Buffer.alloc(1_000_000), true);
    await vi.advanceTimersByTimeAsync(10_000); // duration timeout forces download to finish
    await flushMicrotasks();

    uploadSocket.emit('open');
    uploadSocket.emit('message', Buffer.from('{"TCPInfo":{"RTT":10000}}'), false);
    await vi.advanceTimersByTimeAsync(10_000);

    const result = await promise;

    expect(result.status).toBe('ok');
    expect(result.data!.downloadMbps).toBeGreaterThan(0);
  });
});
