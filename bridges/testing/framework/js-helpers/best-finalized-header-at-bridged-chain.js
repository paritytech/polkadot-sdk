async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const bridgedChainName = args[0];
    const expectedBridgedChainHeaderNumber = Number(args[1]);
    const runtimeApiMethod = bridgedChainName + "FinalityApi_best_finalized";

    while (true) {
        const encodedBestFinalizedHeaderId = await api.rpc.state.call(runtimeApiMethod, []);
        const bestFinalizedHeaderId = api.createType("Option<BpRuntimeHeaderId>", encodedBestFinalizedHeaderId);
        if (bestFinalizedHeaderId.isSome) {
            const bestFinalizedHeaderNumber = Number(bestFinalizedHeaderId.unwrap().toHuman()[0]);
            if (bestFinalizedHeaderNumber > expectedBridgedChainHeaderNumber) {
                return bestFinalizedHeaderNumber;
            }
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
