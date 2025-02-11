async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const relayerAccountAddress = args.relayerAccountAddress;
    const reward_kind = args.rewardKind;
    const expectedRelayerReward = BigInt(args.expectedRelayerReward);
    while (true) {
        const relayerReward = await api.query.bridgeRelayers.relayerRewards(relayerAccountAddress, reward_kind);
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
