import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: [
      'tests/unit/**/*.test.ts',
      'tests/contract/**/*.test.ts',
      'tests/workflow/**/*.test.ts',
    ],
    testTimeout: 60_000,
    sequence: {
      concurrent: false,
    },
  },
});
