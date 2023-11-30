const assert = require("assert");

async function run(nodeName, networkInfo, _jsArgs) {
  const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
  const api = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const calls = [
    // Default broker configuration
    api.tx.broker.configure({
      advanceNotice: 2,
      interludeLength: 1,
      leadinLength: 1,
      regionLength: 3,
      idealBulkProportion: 100,
      limitCoresOffered: null,
      renewalBump: 10,
      contributionTimeout: 5,
    }),
    // Make reservation for ParaId 100 (adder-a) every other block
    // and ParaId 101 (adder-b) every other block.
    api.tx.broker.reserve([
      {
        mask: [255, 0, 255, 0, 255, 0, 255, 0, 255, 0],
        assignment: { Task: 100 },
      },
      {
        mask: [0, 255, 0, 255, 0, 255, 0, 255, 0, 255],
        assignment: { Task: 101 },
      },
    ]),
    // Start sale with 1 core starting at 1 planck
    api.tx.broker.startSales(1, 1),
  ];
  const sudo_batch = api.tx.sudo.sudo(api.tx.utility.batch(calls));

  await new Promise(async (resolve, reject) => {
    const unsub = await sudo_batch.signAndSend(alice, (result) => {
      console.log(`Current status is ${result.status}`);
      if (result.status.isInBlock) {
        console.log(
          `Transaction included at blockHash ${result.status.asInBlock}`
        );
      } else if (result.status.isFinalized) {
        console.log(
          `Transaction finalized at blockHash ${result.status.asFinalized}`
        );
        unsub();
        return resolve();
      } else if (result.isError) {
        console.log(`Transaction Error`);
        unsub();
        return reject();
      }
    });
  });

  return 0;
}

module.exports = { run };
