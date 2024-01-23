async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const exitAfterSeconds = Number(args[0]);
    const account = args[1];
    const expectedNonce = Number(args[2]);

    api.rpc.chain.subscribeFinalizedHeads(async function (header) {
        const apiAtCurrent = await api.at(header.hash);
        const accountState = await apiAtCurrent.query.system.account(account);
        const accountNonce = accountState.nonce.toNumber();
        if (accountNonce >= expectedNonce) {
            console.log("Nonce of account " + account + " is " + accountNonce + " at block " + header.hash);
            process.exit();
        }
    });

    await new Promise(resolve => setTimeout(resolve, exitAfterSeconds * 1000));
    throw new Error("Account transactions are not included into finalized block. Too many reorgs?");
}

module.exports = { run }
