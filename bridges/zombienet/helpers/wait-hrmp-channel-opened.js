async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const sibling = args[0];

    api.rpc.chain.subscribeNewHeads(async function (header) {
        const apiAtCurrent = await api.at(header.hash);
console.log("=== WaitForHrmp: " + new Date().toLocaleString() + " " + header.number);
        const messagingStateAsObj = await apiAtCurrent.query.parachainSystem.relevantMessagingState();
        const messagingState = apiAtCurrent.createType(
            "Option<CumulusPalletParachainSystemRelayStateSnapshotMessagingStateSnapshot>",
            messagingStateAsObj,
        );
        if (messagingState.isSome) {
            const egressChannels = messagingState.unwrap().egressChannels;
            if (egressChannels.find(x => x[0] == sibling)) {
                return;
            }
        }
    }
}

module.exports = { run }
