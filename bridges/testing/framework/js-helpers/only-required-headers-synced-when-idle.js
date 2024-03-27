const utils = require("./utils");

async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // parse arguments
    const exitAfterSeconds = Number(args[0]);
    const bridgedChain = require("./chains/" + args[1]);

    // start listening to new blocks
    let atLeastOneMessageReceived = false;
    let atLeastOneMessageDelivered = false;
    const unsubscribe = await api.rpc.chain.subscribeNewHeads(async function (header) {
        const apiAtParent = await api.at(header.parentHash);
        const apiAtCurrent = await api.at(header.hash);
        const currentEvents = await apiAtCurrent.query.system.events();

        const messagesReceived = currentEvents.find((record) => {
            return record.event.section == bridgedChain.messagesPalletName
                && record.event.method == "MessagesReceived";
        }) != undefined;
        const messagesDelivered = currentEvents.find((record) => {
            return record.event.section == bridgedChain.messagesPalletName &&
                record.event.method == "MessagesDelivered";
        }) != undefined;
        const hasMessageUpdates = messagesReceived || messagesDelivered;
        atLeastOneMessageReceived = atLeastOneMessageReceived || messagesReceived;
        atLeastOneMessageDelivered = atLeastOneMessageDelivered || messagesDelivered;

        if (!hasMessageUpdates) {
            // if there are no any message update transactions, we only expect mandatory GRANDPA
            // headers and initial parachain headers
            await utils.ensureOnlyMandatoryGrandpaHeadersImported(
                bridgedChain,
                apiAtParent,
                apiAtCurrent,
                currentEvents,
            );
            await utils.ensureOnlyInitialParachainHeaderImported(
                bridgedChain,
                apiAtParent,
                apiAtCurrent,
                currentEvents,
            );
        } else {
            const messageTransactions = (messagesReceived ? 1 : 0) + (messagesDelivered ? 1 : 0);

            // otherwise we only accept at most one GRANDPA header
            const newGrandpaHeaders = utils.countGrandpaHeaderImports(bridgedChain, currentEvents);
            if (newGrandpaHeaders > 1) {
                utils.logEvents(currentEvents);
                throw new Error("Unexpected relay chain header import: " + newGrandpaHeaders + " / " + messageTransactions);
            }

            // ...and at most one parachain header
            const newParachainHeaders = utils.countParachainHeaderImports(bridgedChain, currentEvents);
            if (newParachainHeaders > 1) {
                utils.logEvents(currentEvents);
                throw new Error("Unexpected parachain header import: " + newParachainHeaders + " / " + messageTransactions);
            }
        }
    });

    // wait until we have received + delivered messages OR until timeout
    await utils.pollUntil(
        exitAfterSeconds,
        () => { return atLeastOneMessageReceived && atLeastOneMessageDelivered; },
        () => { unsubscribe(); },
        () => {
            if (!atLeastOneMessageReceived) {
                throw new Error("No messages received from bridged chain");
            }
            if (!atLeastOneMessageDelivered) {
                throw new Error("No messages delivered to bridged chain");
            }
        },
    );
}

module.exports = { run }
