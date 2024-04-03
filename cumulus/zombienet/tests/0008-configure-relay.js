const assert = require("assert");

async function run(nodeName, networkInfo, _jsArgs) {
  const init = networkInfo.nodesByName[nodeName];
  let wsUri = init.wsUri;
  let userDefinedTypes = init.userDefinedTypes;
  const api = await zombie.connect(wsUri, userDefinedTypes);

  const collatorElastic = networkInfo.nodesByName["collator-elastic"];
  wsUri = collatorElastic.wsUri;
  userDefinedTypes = collatorElastic.userDefinedTypes;
  const apiCollatorElastic = await zombie.connect(wsUri, userDefinedTypes);

  const collatorSingleCore = networkInfo.nodesByName["collator-single-core"];
  wsUriSingleCore = collatorSingleCore.wsUri;
  userDefinedTypes6s = collatorSingleCore.userDefinedTypes;

  const apiCollatorSingleCore = await zombie.connect(wsUriSingleCore, userDefinedTypes6s);

  await zombie.util.cryptoWaitReady();

  // Get the genesis header and the validation code of parachain 2100
  const genesisHeaderElastic = await apiCollatorElastic.rpc.chain.getHeader();
  const validationCodeElastic = await apiCollatorElastic.rpc.state.getStorage("0x3A636F6465");

  // Get the genesis header and the validation code of parachain 2000
  const genesisHeaderSingleCore = await apiCollatorSingleCore.rpc.chain.getHeader();
  const validationCodeSingleCore = await apiCollatorSingleCore.rpc.state.getStorage("0x3A636F6465");

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const calls = [
    api.tx.configuration.setCoretimeCores({ new: 7 }),
    api.tx.coretime.assignCore(0, 20,[[ { task: 1005 }, 57600 ]], null),
    api.tx.registrar.forceRegister(
      alice.address,
      0,
      2100,
      genesisHeaderElastic.toHex(),
      validationCodeElastic.toHex(),
    ),
    api.tx.registrar.forceRegister(
      alice.address,
      0,
      2000,
      genesisHeaderSingleCore.toHex(),
      validationCodeSingleCore.toHex(),
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
