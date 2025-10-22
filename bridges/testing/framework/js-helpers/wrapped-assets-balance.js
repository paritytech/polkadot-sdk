async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const accountAddress = args.accountAddress;
    const expectedAssetId = args.expectedAssetId;
    const expectedAssetBalance = BigInt(args.expectedAssetBalance);

    while (true) {
        const foreignAssetAccount = await api.query.foreignAssets.account(expectedAssetId, accountAddress);
        if (foreignAssetAccount.isSome) {
            const foreignAssetAccountBalance = foreignAssetAccount.unwrap().balance.toBigInt();
            if (foreignAssetAccountBalance > expectedAssetBalance) {
                return foreignAssetAccountBalance;
            }
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
