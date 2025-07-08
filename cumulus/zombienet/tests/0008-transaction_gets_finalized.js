//based on: https://polkadot.js.org/docs/api/examples/promise/transfer-events

const assert = require("assert");

async function run(nodeName, networkInfo, args) {
  const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
  // Create the API and wait until ready
  var api = null;
  var keyring = null;
  if (zombie == null) {
    const testKeyring = require('@polkadot/keyring/testing');
    const { WsProvider, ApiPromise } = require('@polkadot/api');
    const provider = new WsProvider(wsUri);
    api = await ApiPromise.create({provider});
    // Construct the keyring after the API (crypto has an async init)
    keyring = testKeyring.createTestKeyring({ type: "sr25519" });
  } else {
    keyring = new zombie.Keyring({ type: "sr25519" });
    api = await zombie.connect(wsUri, userDefinedTypes);
  }


  // Add Alice to our keyring with a hard-derivation path (empty phrase, so uses dev)
  const alice = keyring.addFromUri('//Alice');

  // Create an extrinsic:
  const extrinsic = api.tx.system.remark("xxx");

  let extrinsic_success_event = false;
  try {
    await new Promise( async (resolve, reject) => {
      const unsubscribe = await extrinsic
        .signAndSend(alice, { nonce: -1 }, ({ events = [], status }) => {
          console.log('Extrinsic status:', status.type);

          if (status.isInBlock) {
            console.log('Included at block hash', status.asInBlock.toHex());
            console.log('Events:');

            events.forEach(({ event: { data, method, section }, phase }) => {
              console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());

              if (section=="system" && method =="ExtrinsicSuccess") {
                extrinsic_success_event = true;
              }
            });
          } else if (status.isFinalized) {
            console.log('Finalized block hash', status.asFinalized.toHex());
            unsubscribe();
            if (extrinsic_success_event) {
              resolve();
            } else {
              reject("ExtrinsicSuccess has not been seen");
            }
          } else if (status.isError) {
            unsubscribe();
            reject("Extrinsic status.isError");
          }

        });
    });
  } catch (error) {
    assert.fail("Transfer promise failed, error: " + error);
  }

  assert.ok("test passed");
}

module.exports = { run }
