//based on: https://polkadot.js.org/docs/api/examples/promise/transfer-events

const assert = require("assert");

async function run(nodeName, networkInfo, args) {
  // console.log('xxxxxxxxxxx 1');
  const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
  // console.log('xxxxxxxxxxx 2');
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
    // console.log('xxxxxxxxxxx 3');
    api = await zombie.connect(wsUri, userDefinedTypes);
    // console.log('xxxxxxxxxxx 4');
  }


  // Add Alice to our keyring with a hard-derivation path (empty phrase, so uses dev)
  const alice = keyring.addFromUri('//Alice');
  // console.log('xxxxxxxxxxx 5');

  // Create a extrinsic: 
  const extrinsic = api.tx.system.remark("xxx");
  // console.log('xxxxxxxxxxx 6');

  let extrinsic_success_event = false;
  try {
    // console.log('xxxxxxxxxxx 7');
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
  // console.log('xxxxxxxxxxx 8');
  } catch (error) {
    assert.fail("Transfer promise failed, error: " + error);
  }

  assert.ok("test passed");
}

module.exports = { run }
