// Explicitly enabled compiler check: reads sources/declarations, never emits or runs project code.
import { readFileSync } from 'node:fs';

import { inspectProject } from './typescript/check.mjs';

try { console.log(JSON.stringify(inspectProject(JSON.parse(readFileSync(0, 'utf8'))))); }
catch (error) { console.error(`TypeScript check failed: ${error.message}`); process.exitCode = 1; }
