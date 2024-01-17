async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // exit timeout
    const exitAfterSeconds = Number(args[0]);
    const bridgedChain = require("./chains/" + args[1]);

    // start listening to new blocks
    const startTime = process.hrtime()[0];
    let totalGrandpaHeaders = 0;
    let totalParachainHeaders = 0;
    api.rpc.chain.subscribeNewHeads(async function (header) {
        // we need to read some storage at parent block
        const parentHeaderHash = header.parentHash;
        const apiAtParentHeader = await api.at(parentHeaderHash);

        // remember whether we already know bridged parachain header at a parent block
        const bestBridgedParachainHeader = await bridgedChain.bestBridgedParachainInfo(apiAtParentHeader);
        const hasBestBridgedParachainHeader = bestBridgedParachainHeader.isSome;

        // remember id of bridged relay chain GRANDPA authorities set at parent block
        const authoritySetAtParent = await bridgedChain.bestBridgedRelayChainGrandpaAuthoritySet(apiAtParentHeader);
        const authoritySetIdAtParent = authoritySetAtParent["setId"];

        // now read the id of bridged relay chain GRANDPA authorities set at current block
        const headerHash = header.hash;
        const apiAtCurrentHeader = await api.at(headerHash);
        const authoritySetAtCurrent = await bridgedChain.bestBridgedRelayChainGrandpaAuthoritySet(apiAtCurrentHeader);
        const authoritySetIdAtCurrent = authoritySetAtCurrent["setId"];

        // we expect to see:
        // - no more than `authoritySetIdAtCurrent - authoritySetIdAtParent` new GRANDPA headers;
        // - no more than `1` bridged parachain header if there were no parachain header before.
        const maxNewGrandpaHeaders = authoritySetIdAtCurrent - authoritySetIdAtParent;
        const maxNewParachainHeaders = hasBestBridgedParachainHeader ? 0 : 1;
        const headerEvents = await apiAtCurrentHeader.query.system.events();
        let newGrandpaHeaders = 0;
        let newParachainHeaders = 0;
        headerEvents.forEach((record) => {
            const { event } = record;
            if (event.section == bridgedChain.grandpaPalletName && event.method == "UpdatedBestFinalizedHeader") {
                newGrandpaHeaders += 1;
            }
            if (event.section == bridgedChain.parachainsPalletName && event.method == "UpdatedParachainHead") {
                newParachainHeaders +=1;
            }
        });
        totalGrandpaHeaders += newGrandpaHeaders;
        totalParachainHeaders += newParachainHeaders;

        // check that our assumptions are correct
        if (newGrandpaHeaders > maxNewGrandpaHeaders) {
            throw new Error("Unexpected relay chain header import: " + newGrandpaHeaders + " / " + maxNewGrandpaHeaders);
        }
        if (newParachainHeaders > maxNewParachainHeaders) {
            throw new Error("Unexpected parachain header import: " + newParachainHeaders + " / " + maxNewParachainHeaders);
        }
    });

    // wait given time
    await new Promise(resolve => setTimeout(resolve, exitAfterSeconds * 1000));
    // if we haven't seen any new GRANDPA or parachain headers => fail
    if (totalGrandpaHeaders == 0) {
        throw new Error("No bridged relay chain headers imported");
    }
    if (totalParachainHeaders == 0) {
        throw new Error("No bridged parachain headers imported");
    }
    // else => everything is ok
    return 1;
}

module.exports = { run }
