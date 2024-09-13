async function run(nodeName, networkInfo, args) {
  const init = networkInfo.nodesByName[nodeName];
  let wsUri = init.wsUri;
  let userDefinedTypes = init.userDefinedTypes;
  const api = await zombie.connect(wsUri, userDefinedTypes);

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  let calls = [];

  for (let i = 0; i < args.length; i++) {
    let para = args[i];
    const sec = networkInfo.nodesByName["collator-" + para];
    const api_collator = await zombie.connect(sec.wsUri, sec.userDefinedTypes);

    await zombie.util.cryptoWaitReady();

    // Get the genesis header and the validation code of the parachain
    const genesis_header = await api_collator.rpc.chain.getHeader();
    const validation_code = await api_collator.rpc.state.getStorage("0x3A636F6465");

    calls.push(
      api.tx.paras.addTrustedValidationCode(validation_code.toHex())
    );
    calls.push(
      api.tx.registrar.forceRegister(
        alice.address,
        0,
        Number(para),
        genesis_header.toHex(),
        validation_code.toHex(),
      )
    );
  }

  const sudo_batch = api.tx.sudo.sudo(api.tx.utility.batch(calls));

  await new Promise(async (resolve, reject) => {
    const unsub = await sudo_batch
      .signAndSend(alice, ({ status, isError }) => {
        if (status.isInBlock) {
          console.log(
            `Transaction included at blockhash ${status.asInBlock}`,
          );
        } else if (status.isFinalized) {
          console.log(
            `Transaction finalized at blockHash ${status.asFinalized}`,
          );
          unsub();
          return resolve();
        } else if (isError) {
          console.log(`Transaction error`);
          reject(`Transaction error`);
        }
      });
  });

  return 0;
}

module.exports = { run };
