async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    args.forEach(async (wsPort) => {
        const peerWsUri = "ws://127.0.0.1:" + wsPort;
        const peerApi = await zombie.connect(peerWsUri, {});
        const peerAddresses = await peerApi.rpc.system.localListenAddresses();
        console.log("Connecting " + wsUri + " to " + peerAddresses[0]);
        await api.rpc.system.addReservedPeer(peerAddresses[0]);
    });

}

module.exports = { run }
