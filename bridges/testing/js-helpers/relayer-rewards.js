async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const relayerAccountAddress = args[0];
    const laneId = args[1];
    const bridgedChainId = args[2];
    const relayerFundOwner = args[3];
    const expectedRelayerReward = BigInt(args[4]);
    while (true) {
        const relayerReward = await api.query.bridgeRelayers.relayerRewards(
            relayerAccountAddress,
            { laneId: laneId, bridgedChainId: bridgedChainId, owner: relayerFundOwner }
        );
        if (relayerReward.isSome) {
            const relayerRewardBalance = relayerReward.unwrap().toBigInt();
            if (relayerRewardBalance > expectedRelayerReward) {
                return relayerRewardBalance;
            }
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
