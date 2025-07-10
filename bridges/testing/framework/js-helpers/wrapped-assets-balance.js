async function run(nodeName, networkInfo, args) {
    const fs = require('fs');
    
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const accountAddress = args.accountAddress;
    const expectedAssetId = args.expectedAssetId;
    const expectedAssetIdString = expectedAssetId.toString();
    const expectedAssetBalance = BigInt(args.expectedAssetBalance);

    // Open the logging file
    const logFile = fs.createWriteStream('asdf.txt', { flags: 'a' });
    logFile.write(`Waiting for asset ${expectedAssetIdString} balance to be greater than ${expectedAssetBalance} for account ${accountAddress} and expectedAssetId ${expectedAssetId}\n`);
    while (true) {
        const foreignAssetAccount = await api.query.foreignAssets.account(expectedAssetId, accountAddress);
        if (foreignAssetAccount.isSome) {
            const foreignAssetAccountBalance = foreignAssetAccount.unwrap().balance.toBigInt();

            // Log the current balance
            logFile.write(`Current balance for asset ${expectedAssetIdString} on account ${accountAddress}: ${foreignAssetAccountBalance}\n`);
            
            if (foreignAssetAccountBalance > expectedAssetBalance) {
                return foreignAssetAccountBalance;
            }
        } else {
            // Log that the asset account is not found
            logFile.write(`Asset account for asset ${expectedAssetIdString} on account ${accountAddress} and assetId ${expectedAssetId} not found\n`);
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
