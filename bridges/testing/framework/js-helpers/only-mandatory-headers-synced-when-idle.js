const utils = require("./utils");

async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // parse arguments
    const exitAfterSeconds = Number(args[0]);
    const bridgedChain = require("./chains/" + args[1]);

    // start listening to new blocks
    let totalGrandpaHeaders = 0;
    let initialParachainHeaderImported = false;
    api.rpc.chain.subscribeNewHeads(async function (header) {
        const apiAtParent = await api.at(header.parentHash);
        const apiAtCurrent = await api.at(header.hash);
        const currentEvents = await apiAtCurrent.query.system.events();

        totalGrandpaHeaders += await utils.ensureOnlyMandatoryGrandpaHeadersImported(
            bridgedChain,
            apiAtParent,
            apiAtCurrent,
            currentEvents,
        );
        initialParachainHeaderImported = await utils.ensureOnlyInitialParachainHeaderImported(
            bridgedChain,
            apiAtParent,
            apiAtCurrent,
            currentEvents,
        );
    });

    // wait given time
    await new Promise(resolve => setTimeout(resolve, exitAfterSeconds * 1000));
    // if we haven't seen any new GRANDPA or parachain headers => fail
    if (totalGrandpaHeaders == 0) {
        throw new Error("No bridged relay chain headers imported");
    }
    if (!initialParachainHeaderImported) {
        throw new Error("No bridged parachain headers imported");
    }
}

module.exports = { run }
