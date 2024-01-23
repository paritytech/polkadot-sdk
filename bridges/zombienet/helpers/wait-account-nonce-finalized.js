async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const account = args[0];
    const expectedNonce = Number(args[1]);
    
    api.rpc.chain.subscribeFinalizedHeads(async function (header) {
        const accountState = await api.query.system.account(account);
        if (accountState.nonce.toNumber() >= expectedNonce) {
            process.exit();
        }
    });
}

module.exports = { run }
