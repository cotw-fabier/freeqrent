// A worker for test/api.test.mjs: its own instance of the package, rendering
// the same style a few times.
import { parentPort, workerData } from 'node:worker_threads';

import { render } from '../../dist/node.mjs';

const { text, style, rounds } = workerData;
const out = [];
for (let i = 0; i < rounds; i++) out.push(render(text, style));
parentPort.postMessage(out);
