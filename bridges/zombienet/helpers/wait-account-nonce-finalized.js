const utils = require("./utils");

async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // parse arguments
    const exitAfterSeconds = Number(args[0]);
    const account = args[1];
    const expectedNonce = Number(args[2]);

    // start listening to new finalized blocks
    let nonceMatches = false;
    const unsubscribe = await api.rpc.chain.subscribeFinalizedHeads(async function (header) {
        const apiAtCurrent = await api.at(header.hash);
        const accountState = await apiAtCurrent.query.system.account(account);
        const accountNonce = accountState.nonce.toNumber();
        console.log("Nonce of account " + account + " is " + accountNonce + " (expected " + expectedNonce + ") at block " + header.hash);
        if (accountNonce >= expectedNonce) {
            nonceMatches = true;
        }
    });

    // wait until we have received + delivered messages OR until timeout
    await utils.pollUntil(
        exitAfterSeconds,
        () => { return nonceMatches; },
        () => { unsubscribe(); },
        () => { throw new Error("Account transactions are not included into finalized block. Too many reorgs?"); },
    );
}

module.exports = { run }
