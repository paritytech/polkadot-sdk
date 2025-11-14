#!/usr/bin/env node

/*
Assign N relay cores to a parachain via sudo on the relaychain.

Usage:
  node scripts/assign_cores.js \
    --ws ws://127.0.0.1:9944 \
    --para 1000 \
    --cores 11 \
    --start-core 0

Notes:
- Run after the parachain is up and producing blocks.
- Requires sudo on the relay (Alice on rococo-local).
*/

(async () => {
  const { ApiPromise, WsProvider, Keyring } = await import('@polkadot/api');

  // Basic argv parsing (no deps)
  const argv = process.argv.slice(2);
  function getArg(name, def) {
    const i = argv.indexOf(`--${name}`);
    if (i !== -1 && i + 1 < argv.length) return argv[i + 1];
    return def;
  }

  const ws = getArg('ws', process.env.WS || 'ws://127.0.0.1:9964');
  const paraId = parseInt(getArg('para', process.env.PARA_ID || '1000'), 10);
  const cores = parseInt(getArg('cores', process.env.CORES || '11'), 10);
  const startCore = parseInt(getArg('start-core', process.env.START_CORE || '0'), 10);
  const beginOverride = getArg('begin', process.env.BEGIN);

  if (!Number.isInteger(paraId) || paraId <= 0) {
    console.error('Invalid --para');
    process.exit(1);
  }
  if (!Number.isInteger(cores) || cores <= 0) {
    console.error('Invalid --cores');
    process.exit(1);
  }
  if (!Number.isInteger(startCore) || startCore < 0) {
    console.error('Invalid --start-core');
    process.exit(1);
  }

  console.log(`Connecting to relay RPC: ${ws}`);
  const api = await ApiPromise.create({ provider: new WsProvider(ws) });

  if (!api?.tx?.coretime?.assignCore) {
    console.error('Runtime does not expose coretime.assignCore. Are you on a recent rococo-local?');
    process.exit(1);
  }
  if (!api?.tx?.utility?.batch) {
    console.error('Runtime does not expose utility.batch.');
    process.exit(1);
  }
  if (!api?.tx?.sudo?.sudo) {
    console.error('Runtime does not expose sudo.sudo.');
    process.exit(1);
  }

  // Determine a valid `begin` height: use override or current relay best.
  let beginHeight;
  if (beginOverride !== undefined) {
    beginHeight = parseInt(beginOverride, 10);
    if (!Number.isInteger(beginHeight) || beginHeight < 0) {
      console.error('Invalid --begin');
      process.exit(1);
    }
  } else {
    const hdr = await api.rpc.chain.getHeader();
    beginHeight = hdr.number.toNumber();
  }

  // Build assignCore calls: assignment=[(Task(paraId), FULL)], end_hint=None
  const FULL = 57600; // PartsOf57600::FULL
  const calls = [];
  for (let i = startCore; i < startCore + cores; i++) {
    const call = api.tx.coretime.assignCore(i, beginHeight, [[{ Task: paraId }, FULL]], null);
    calls.push(call);
  }

  const batched = api.tx.utility.batch(calls);

  // Sign with Alice (sudo key on rococo-local)
  const keyring = new Keyring({ type: 'sr25519' });
  const alice = keyring.addFromUri('//Alice');

  console.log(`Submitting sudo(batch(assignCore)) for para ${paraId} on cores [${startCore}..${startCore + cores - 1}]`);
  // Prepare signing options to avoid mortal-era unknown-block issues after restarts.
  await new Promise((resolve, reject) => {
    api.tx.sudo
      .sudo(batched)
      // Use fresh connection and default (mortal) era; avoid passing era/blockHash combo.
      .signAndSend(alice, { nonce: -1 }, ({ status, dispatchError, events }) => {
        if (dispatchError) {
          if (dispatchError.isModule) {
            const decoded = api.registry.findMetaError(dispatchError.asModule);
            console.error('Dispatch error:', `${decoded.section}.${decoded.name}`, decoded.docs.join(' '));
          } else {
            console.error('Dispatch error:', dispatchError.toString());
          }
          reject(new Error('Dispatch failed'));
          return;
        }
        if (status.isInBlock) {
          console.log(`Included in block ${status.asInBlock.toString()}`);
        } else if (status.isFinalized) {
          const block = status.asFinalized.toString();
          // Surface Utility.BatchInterrupted details if present
          for (const { event } of events) {
            if (api.events.utility && api.events.utility.BatchInterrupted.is(event)) {
              const { index, error } = event.data;
              console.error('BatchInterrupted at index', index.toString());
              if (error.isModule) {
                const e = api.registry.findMetaError(error.asModule);
                console.error('Error:', `${e.section}.${e.name}`, e.docs.join(' '));
              } else {
                console.error('Error:', error.toString());
              }
            }
          }
          const success = events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event));
          if (success) console.log(`Finalized in block ${block}`);
          else console.warn(`Finalized in block ${block} (no ExtrinsicSuccess found)`);
          resolve();
        }
      })
      .catch(reject);
  });

  await api.disconnect();
  console.log('All done.');
  process.exit(0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
