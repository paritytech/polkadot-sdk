const assert = require("assert");

async function run(nodeName, networkInfo, _jsArgs) {
  const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
  const api = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const calls = [
    api.tx.configuration.setCoretimeCores({ new: 1 }),
    api.tx.coretime.assignCore(0, 20,[[ { task: 1005 }, 57600 ]], null)
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
