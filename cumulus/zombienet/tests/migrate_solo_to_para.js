const assert = require("assert");
const polkadotApi = require("@polkadot/api");
const utilCrypto = require("@polkadot/util-crypto");
const fs = require("fs").promises;

async function connect(apiUrl, types) {
    const provider = new polkadotApi.WsProvider(apiUrl);
    const api = new polkadotApi.ApiPromise({ provider, types });
    await api.isReady;
    return api;
}

async function run(nodeName, networkInfo, args) {
    const [paraNode, partialPath, soloNode ] = args;
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[paraNode];
    const {wsUri: wsUri_solo, userDefinedTypes: userDefinedTypes_solo } = networkInfo.nodesByName[soloNode];
    const para = await connect(wsUri, userDefinedTypes);
    const solo = await connect(wsUri_solo, userDefinedTypes_solo);

    await utilCrypto.cryptoWaitReady();

    // account to submit tx
    const keyring = new polkadotApi.Keyring({ type: "sr25519" });
    const alice = keyring.addFromUri("//Alice");

    // get genesis to update
    const filePath = `${networkInfo.tmpDir}/${partialPath}/genesis-state`;
    const customHeader = await fs.readFile(filePath);

    // get current header
    await para.tx.testPallet.setCustomValidationHeadData(customHeader.toString()).signAndSend(alice);

    let parachain_best;
    let count = 0;

    assertParachainBest = async (parachain_best) => {
        const current = await para.rpc.chain.getHeader();
        assert.equal(parachain_best.toHuman().number, current.toHuman().number, "parachain should not produce more blocks");
    }


    await new Promise( async (resolve, reject) => {
        const unsubscribe = await solo.rpc.chain.subscribeNewHeads(async (header) => {
            console.log(`Solo chain is at block: #${header.number}`);
            count++;
            if(count === 2) parachain_best = await para.rpc.chain.getHeader();

            if(count > 4) {
                unsubscribe();
                await assertParachainBest(parachain_best);
                resolve();
            }
        });
    });
}

module.exports = { run }