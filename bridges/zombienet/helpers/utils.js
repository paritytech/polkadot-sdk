module.exports = {
    countGrandpaHeaderImports: function(bridgedChain, events) {
        return events.reduce(
            (count, record) => {
                const { event } = record;
                if (event.section == bridgedChain.grandpaPalletName && event.method == "UpdatedBestFinalizedHeader") {
                    count += 1;
                }
                return count;
            },
            0,
        );
    },
    countParachainHeaderImports: function(bridgedChain, events) {
        return events.reduce(
            (count, record) => {
                const { event } = record;
                if (event.section == bridgedChain.parachainsPalletName && event.method == "UpdatedParachainHead") {
                    count += 1;
                }
                return count;
            },
            0,
        );
    },
    ensureOnlyMandatoryGrandpaHeadersImported: async function(
        bridgedChain,
        apiAtParent,
        apiAtCurrent,
        currentEvents,
    ) {
        // remember id of bridged relay chain GRANDPA authorities set at parent block
        const authoritySetAtParent = await bridgedChain.bestBridgedRelayChainGrandpaAuthoritySet(apiAtParent);
        const authoritySetIdAtParent = authoritySetAtParent["setId"];

        // now read the id of bridged relay chain GRANDPA authorities set at current block
        const authoritySetAtCurrent = await bridgedChain.bestBridgedRelayChainGrandpaAuthoritySet(apiAtCurrent);
        const authoritySetIdAtCurrent = authoritySetAtCurrent["setId"];

        // we expect to see no more than `authoritySetIdAtCurrent - authoritySetIdAtParent` new GRANDPA headers
        const maxNewGrandpaHeaders = authoritySetIdAtCurrent - authoritySetIdAtParent;
        const newGrandpaHeaders = module.exports.countGrandpaHeaderImports(bridgedChain, currentEvents);

        // check that our assumptions are correct
console.log("=== " + bridgedChain.grandpaPalletName + ": " + newGrandpaHeaders + " / " + maxNewGrandpaHeaders);
        if (newGrandpaHeaders > maxNewGrandpaHeaders) {
            throw new Error("Unexpected relay chain header import: " + newGrandpaHeaders + " / " + maxNewGrandpaHeaders);
        }

        return newGrandpaHeaders;
    },
    ensureOnlyInitialParachainHeaderImported: async function(
        bridgedChain,
        apiAtParent,
        apiAtCurrent,
        currentEvents,
    ) {
        // remember whether we already know bridged parachain header at a parent block
        const bestBridgedParachainHeader = await bridgedChain.bestBridgedParachainInfo(apiAtParent);
        const hasBestBridgedParachainHeader = bestBridgedParachainHeader.isSome;

        // we expect to see: no more than `1` bridged parachain header if there were no parachain header before.
        const maxNewParachainHeaders = hasBestBridgedParachainHeader ? 0 : 1;
        const newParachainHeaders = module.exports.countParachainHeaderImports(bridgedChain, currentEvents);

        // check that our assumptions are correct
console.log("=== " + bridgedChain.parachainsPalletName + ": " + newParachainHeaders + " / " + maxNewParachainHeaders);
        if (newParachainHeaders > maxNewParachainHeaders) {
            throw new Error("Unexpected parachain header import: " + newParachainHeaders + " / " + maxNewParachainHeaders);
        }

        return newParachainHeaders;
    },
}
