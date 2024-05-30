async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const accountAddress = args[0];
    const expectedForeignAssetBalance = BigInt(args[1]);
    const bridgedNetworkName = args[2];
    while (true) {
        const foreignAssetAccount = await api.query.foreignAssets.account(
            { parents: 2, interior: { X1: { GlobalConsensus: bridgedNetworkName } } },
            accountAddress
        );
        if (foreignAssetAccount.isSome) {
            const foreignAssetAccountBalance = foreignAssetAccount.unwrap().balance.toBigInt();
            if (foreignAssetAccountBalance > expectedForeignAssetBalance) {
                return foreignAssetAccountBalance;
            }
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
