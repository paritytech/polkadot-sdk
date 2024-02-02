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
      advanceNotice: 5,
      interludeLength: 1,
      leadinLength: 1,
      regionLength: 1,
      idealBulkProportion: 100,
      limitCoresOffered: null,
      renewalBump: 10,
      contributionTimeout: 5,
    }),
    // We need MOARE cores.
    api.tx.broker.requestCoreCount(2),
    // Set a lease for the broker chain itself.
    api.tx.broker.setLease(
      1005,
      1000,
    ),
    // Set a lease for parachain 100
    api.tx.broker.setLease(
      100,
      1000,
    ),
    // Start sale to make the broker "work", but we don't offer any cores
    // as we have fixed leases only anyway.
    api.tx.broker.startSales(1, 0),
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
        // Probably happens because of: https://github.com/paritytech/polkadot-sdk/issues/1202.
        console.log(`Transaction error`);
        // We ignore the error because it is very likely misleading, because of the issue mentioned above.
        unsub();
        return resolve();
      }
    });
  });

  return 0;
}

module.exports = { run };
