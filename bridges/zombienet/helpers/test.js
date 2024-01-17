#!node
const { WsProvider, ApiPromise } = require("@polkadot/api");
const util = require("@polkadot/util");
const bridgedChain = require("./chains/rococo-at-westend.js");

async function connect(endpoint, types = {}) {
    const provider = new WsProvider(endpoint);
    const api = await ApiPromise.create({
        provider,
        types: {/*
            HeaderId: {
                number: "u32",
                hash: "H256"
            }*/
        },
        throwOnConnect: false,
    });
    return api;
}

async function test() {
    const api = await connect("ws://127.0.0.1:8945");
//    const api = await connect("wss://westend-bridge-hub-rpc.dwellir.com");
    const exitAfterSeconds = 600;

    console.warn("=== 2: " + exitAfterSeconds);
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
        if (newGrandpaHeaders > maxNewGrandpaHeaders || newParachainHeaders > maxNewParachainHeaders) {
            return 0;
        }

        // exit if we have waited for too long
        if (process.hrtime()[0] - startTime > exitAfterSeconds) {
            // if we haven't seen any new GRANDPA or parachain headers => fail
            if (totalGrandpaHeaders == 0 || totalParachainHeaders == 0) {
                return 0;
            }
            // else => everything is ok
            return 1;
        }
    });
}

test()