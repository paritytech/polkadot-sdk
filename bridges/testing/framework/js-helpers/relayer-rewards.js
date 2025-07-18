async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // TODO: could be replaced with https://github.com/polkadot-js/api/issues/4930 (depends on metadata v15) later
    const relayerAccountAddress = args.relayerAccountAddress;
    const reward = args.reward;
    const expectedRelayerReward = BigInt(args.expectedRelayerReward);
    console.log("Waiting rewards for relayerAccountAddress: " + relayerAccountAddress + " expecting minimal rewards at least: " + expectedRelayerReward + " for " + JSON.stringify(reward));
    while (true) {
        const relayerReward = await api.query.bridgeRelayers.relayerRewards(relayerAccountAddress, reward);
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
