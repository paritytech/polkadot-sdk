const assert = require("assert");

async function run(nodeName, networkInfo, _jsArgs) {
  const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
  const api = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // The billing account:
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const paraId = 2000;
  const setBillingAccountCall = 
    api.tx.registrar.forceSetParachainBillingAccount(paraId, alice.address);
  const sudoCall = api.tx.sudo.sudo(setBillingAccountCall);

  await new Promise(async (resolve, reject) => {
    const unsub = await sudoCall.signAndSend(alice, (result) => {
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
