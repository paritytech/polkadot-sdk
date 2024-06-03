// Assign a parachain to a core.
async function run(nodeName, networkInfo, args) {
  const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
  const api = await zombie.connect(wsUri, userDefinedTypes);

  await zombie.util.cryptoWaitReady();

  // Submit transaction with Alice accoung
  const keyring = new zombie.Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

    // Wait for this transaction to be finalized in a block.
    await new Promise(async (resolve, reject) => {
      const unsub = await api.tx.sudo
        .sudo(api.tx.system.killPrefix("0x638595eebaa445ce03a13547bece90e704e6ac775a3245623103ffec2cb2c92f", 10))
        .signAndSend(alice, ({ status, isError }) => {
          if (status.isInBlock) {
            console.log(
              `killPrefix transaction included at blockhash ${status.asInBlock}`,
            );
          } else if (status.isFinalized) {
            console.log(
              `killPrefix transaction finalized at blockHash ${status.asFinalized}`,
            );
            unsub();
            return resolve();
          } else if (isError) {
            console.log(`killPrefix error`);
            reject(`killPrefix error`);
          }
        });
    });

  // Wait for this transaction to be finalized in a block.
  await new Promise(async (resolve, reject) => {
    const unsub = await api.tx.sudo
      .sudo(api.tx.coretime.assignCore(0, 0, [[{ task: 2000 }, 28800], [{ task: 2001 }, 28800]], null))
      .signAndSend(alice, ({ status, isError }) => {
        if (status.isInBlock) {
          console.log(
            `assignCore transaction included at blockhash ${status.asInBlock}`,
          );
        } else if (status.isFinalized) {
          console.log(
            `assignCore transaction finalized at blockHash ${status.asFinalized}`,
          );
          unsub();
          return resolve();
        } else if (isError) {
          console.log(`assignCore error`);
          reject(`assignCore error`);
        }
      });
  });

  return 0;
}

module.exports = { run };
