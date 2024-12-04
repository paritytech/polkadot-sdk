const assert = require("assert");

async function run(nodeName, networkInfo, _jsArgs) {
  const init = networkInfo.nodesByName[nodeName];
  let wsUri = init.wsUri;
  let userDefinedTypes = init.userDefinedTypes;
  const api = await zombie.connect(wsUri, userDefinedTypes);

  const sec = networkInfo.nodesByName["collator-para-100"];
  wsUri = sec.wsUri;
  userDefinedTypes = sec.userDefinedTypes;

  const api_collator = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // Get the genesis header and the validation code of parachain 100
  const genesis_header = await api_collator.rpc.chain.getHeader();
  const validation_code = await api_collator.rpc.state.getStorage("0x3A636F6465");

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const calls = [
    api.tx.configuration.setCoretimeCores({ new: 1 }),
    api.tx.coretime.assignCore(0, 20,[[ { task: 1005 }, 57600 ]], null),
    api.tx.registrar.forceRegister(
      alice.address,
      0,
      100,
      genesis_header.toHex(),
      validation_code.toHex(),
    )
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
