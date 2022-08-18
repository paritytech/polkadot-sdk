const assert = require("assert");

async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    // get blockhash/runtimeVersion at block 1
    const hashAtBlock1 = await api.rpc.chain.getBlockHash(1);
    const versionAtBlock1 = await api.rpc.state.getRuntimeVersion(hashAtBlock1.toHuman());

    // get blockhash/runtimeVersion at current head
    const currentHeader = await api.rpc.chain.getHeader();
    const hashAtCurrent = await api.rpc.chain.getBlockHash(currentHeader.number.toHuman());
    const versionAtCurrent = await api.rpc.state.getRuntimeVersion(hashAtCurrent.toHuman());

    const oldVersionIncremented = parseInt(versionAtBlock1.specVersion.toHuman(),10) + 1;
    console.log("current", versionAtCurrent.specVersion.toHuman());
    console.log("oldVersionIncremented", oldVersionIncremented);

    // 2 == 2
    assert.equal( oldVersionIncremented, versionAtCurrent.specVersion.toHuman(), "Running version should be the incremented version");
}

module.exports = { run }