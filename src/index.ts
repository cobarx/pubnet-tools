#!/usr/bin/env -S node --import tsx/esm
import { buildCli } from './cli.js';

await buildCli().parseAsync(process.argv);
