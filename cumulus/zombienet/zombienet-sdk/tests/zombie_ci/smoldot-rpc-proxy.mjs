// Smoldot WebSocket RPC Proxy for E2E testing
//
// Starts a smoldot light client connected to a local zombienet network and
// exposes a WebSocket JSON-RPC server. Each WebSocket connection gets its own
// smoldot chain instances with the statement protocol enabled.
//
// Config via environment variables:
//   RELAY_CHAIN_SPEC  - path to relay chain spec JSON file
//   PARA_CHAIN_SPEC   - path to parachain spec JSON file (optional)
//   SMOLDOT_PORT      - WebSocket listen port (default: 9944)
//   SMOLDOT_JS_PATH   - path to smoldot wasm-node/javascript directory

import { createRequire } from 'node:module';
import { Worker } from 'node:worker_threads';
import * as fs from 'node:fs';
import process from 'node:process';
import * as path from 'node:path';

const smoldotJsPath = process.env.SMOLDOT_JS_PATH;
if (!smoldotJsPath) {
    console.error('Error: SMOLDOT_JS_PATH environment variable not set');
    process.exit(1);
}

// Use createRequire to resolve 'ws' from the smoldot JS directory
// (ESM resolves bare specifiers relative to the importing file, not cwd)
const require = createRequire(path.join(smoldotJsPath, 'package.json'));
const { WebSocketServer } = require('ws');

const smoldot = await import(path.join(smoldotJsPath, 'dist/mjs/index-nodejs.js'));
const workerPath = path.join(smoldotJsPath, 'demo/demo-worker.mjs');

if (!process.env.RELAY_CHAIN_SPEC) {
    console.error('Error: RELAY_CHAIN_SPEC environment variable not set');
    process.exit(1);
}

const relaySpec = fs.readFileSync(process.env.RELAY_CHAIN_SPEC, 'utf8');
const paraSpec = process.env.PARA_CHAIN_SPEC
    ? fs.readFileSync(process.env.PARA_CHAIN_SPEC, 'utf8')
    : null;
const port = parseInt(process.env.SMOLDOT_PORT || '9944');

// Start smoldot with a worker thread
const { port1, port2 } = new MessageChannel();
const worker = new Worker(workerPath);
worker.on('error', (err) => {
    console.error('Worker error:', err.message, err.stack);
});
worker.postMessage(port2, [port2]);

const client = smoldot.start({
    portToWorker: port1,
    maxLogLevel: 4,
    forbidTcp: false,
    forbidWs: false,
    forbidNonLocalWs: false,
    forbidWss: false,
    logCallback: (_level, target, message) => {
        const now = new Date();
        const ts = now.toISOString().substring(11, 23);
        console.log(`[${ts}] [${target}] ${message}`);
    }
});

// Start WebSocket server
const wsServer = new WebSocketServer({ port });

// Signal readiness to the parent process
console.log(`SMOLDOT_READY port=${port}`);

wsServer.on('connection', async (connection) => {
    console.log('New RPC client connected');

    try {
        // Each connection gets its own chain instances
        const relayChain = await client.addChain({
            chainSpec: relaySpec,
            disableJsonRpc: true,
            statementStore: { maxSeenStatements: 65536, falsePositiveRate: 0.01 },
        });

        let target = relayChain;
        let paraChain = null;

        if (paraSpec) {
            paraChain = await client.addChain({
                chainSpec: paraSpec,
                potentialRelayChains: [relayChain],
                statementStore: { maxSeenStatements: 65536, falsePositiveRate: 0.01 },
            });
            target = paraChain;
        }

        // Forward JSON-RPC responses from smoldot to the WebSocket client
        (async () => {
            try {
                for await (const response of target.jsonRpcResponses) {
                    connection.send(response);
                }
            } catch (_e) {}
        })();

        // Forward JSON-RPC requests from the WebSocket client to smoldot
        connection.on('message', (data, isBinary) => {
            if (!isBinary) {
                target.sendJsonRpc(data.toString('utf8'));
            } else {
                connection.close(1002); // Protocol error
            }
        });

        // Clean up on disconnect
        connection.on('close', () => {
            console.log('RPC client disconnected');
            try {
                if (paraChain) paraChain.remove();
                relayChain.remove();
            } catch (_e) {}
        });
    } catch (error) {
        console.error('Error adding chain:', error);
        connection.close(1011); // Internal server error
    }
});

process.on('SIGINT', () => process.exit(0));
process.on('SIGTERM', () => process.exit(0));
