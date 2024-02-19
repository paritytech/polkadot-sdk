module.exports = {
    logEvents: function(events) {
        let stringifiedEvents = "";
        events.forEach((record) => {
            if (stringifiedEvents != "") {
                stringifiedEvents += ", ";
            }
            stringifiedEvents += record.event.section + "::" + record.event.method;
        });
        console.log("Block events: " + stringifiedEvents);
    },
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
    pollUntil: async function(
        timeoutInSecs,
        predicate,
        cleanup,
        onFailure,
    )  {
        const begin = new Date().getTime();
        const end = begin + timeoutInSecs * 1000;
        while (new Date().getTime() < end) {
            if (predicate()) {
                cleanup();
                return;
            }
            await new Promise(resolve => setTimeout(resolve, 100));
        }

        cleanup();
        onFailure();
    },
    ensureOnlyMandatoryGrandpaHeadersImported: async function(
        bridgedChain,
        apiAtParent,
        apiAtCurrent,
        currentEvents,
    ) {
        // remember id of bridged relay chain GRANDPA authorities set at parent block
        const authoritySetAtParent = await apiAtParent.query[bridgedChain.grandpaPalletName].currentAuthoritySet();
        const authoritySetIdAtParent = authoritySetAtParent["setId"];

        // now read the id of bridged relay chain GRANDPA authorities set at current block
        const authoritySetAtCurrent = await apiAtCurrent.query[bridgedChain.grandpaPalletName].currentAuthoritySet();
        const authoritySetIdAtCurrent = authoritySetAtCurrent["setId"];

        // we expect to see no more than `authoritySetIdAtCurrent - authoritySetIdAtParent` new GRANDPA headers
        const maxNewGrandpaHeaders = authoritySetIdAtCurrent - authoritySetIdAtParent;
        const newGrandpaHeaders = module.exports.countGrandpaHeaderImports(bridgedChain, currentEvents);

        // check that our assumptions are correct
        if (newGrandpaHeaders > maxNewGrandpaHeaders) {
            module.exports.logEvents(currentEvents);
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
        const bestBridgedParachainHeader = await apiAtParent.query[bridgedChain.parachainsPalletName].parasInfo(bridgedChain.bridgedBridgeHubParaId);;
        const hasBestBridgedParachainHeader = bestBridgedParachainHeader.isSome;

        // we expect to see: no more than `1` bridged parachain header if there were no parachain header before.
        const maxNewParachainHeaders = hasBestBridgedParachainHeader ? 0 : 1;
        const newParachainHeaders = module.exports.countParachainHeaderImports(bridgedChain, currentEvents);

        // check that our assumptions are correct
        if (newParachainHeaders > maxNewParachainHeaders) {
            module.exports.logEvents(currentEvents);
            throw new Error("Unexpected parachain header import: " + newParachainHeaders + " / " + maxNewParachainHeaders);
        }

        return hasBestBridgedParachainHeader;
    },
}
