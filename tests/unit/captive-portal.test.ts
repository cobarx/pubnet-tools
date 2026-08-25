import { describe, expect, test } from 'vitest';
import { classifyCaptivePortal } from '../../src/checks/security.js';

const EXPECT_204 = { expectedStatus: 204 };
const EXPECT_200_SUCCESS = { expectedStatus: 200, expectedBodyContains: 'Success' };

describe('classifyCaptivePortal', () => {
  // spec: captive-portal-detection#S1
  test('an unmodified 204 response is not a portal', () => {
    const result = classifyCaptivePortal(
      { status: 204, location: null, body: '' },
      EXPECT_204
    );
    expect(result.detected).toBe(false);
    expect(result.method).toBe('none');
  });

  test('an unmodified 200 body match is not a portal', () => {
    const result = classifyCaptivePortal(
      { status: 200, location: null, body: '<HTML><BODY>Success</BODY></HTML>' },
      EXPECT_200_SUCCESS
    );
    expect(result.detected).toBe(false);
    expect(result.method).toBe('none');
  });

  // spec: captive-portal-detection#S2
  test('a redirect away from the canary is a portal, redirect method', () => {
    const result = classifyCaptivePortal(
      { status: 302, location: 'http://portal.example.com/login', body: '' },
      EXPECT_204
    );
    expect(result.detected).toBe(true);
    expect(result.method).toBe('redirect');
    expect(result.redirectLocation).toBe('http://portal.example.com/login');
  });

  // spec: captive-portal-detection#S3
  test('the expected status with substituted content is a portal, content-mismatch method', () => {
    const result = classifyCaptivePortal(
      { status: 200, location: null, body: '<HTML><BODY>Please log in</BODY></HTML>' },
      EXPECT_200_SUCCESS
    );
    expect(result.detected).toBe(true);
    expect(result.method).toBe('content-mismatch');
  });

  test('an unreachable canary is not reported as a detected portal', () => {
    const result = classifyCaptivePortal({ status: null, location: null, body: '' }, EXPECT_204);
    expect(result.detected).toBe(false);
    expect(result.method).toBe('none');
  });
});
