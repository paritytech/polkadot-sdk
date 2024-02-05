async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const sibling = args[0];

    while (true) {
        const messagingStateAsObj = await api.query.parachainSystem.relevantMessagingState();
        const messagingState = api.createType("Option<CumulusPalletParachainSystemRelayStateSnapshotMessagingStateSnapshot>", messagingStateAsObj);
        if (messagingState.isSome) {
            const egressChannels = messagingState.unwrap().egressChannels;
            if (egressChannels.find(x => x[0] == sibling)) {
                return;
            }
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 12000));
    }
}

module.exports = { run }
