async function run(nodeName, networkInfo, _jsArgs) {
  const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
  const api = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // account to submit tx
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  await new Promise(async (resolve, reject) => {
    const unsub = await api.tx.sudo
      .sudo(api.tx.coretime.assignCore(0, 35, [[{ task: 2000 }, 57600]], null))
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
