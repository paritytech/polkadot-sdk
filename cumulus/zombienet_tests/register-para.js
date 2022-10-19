async function run(nodeName, networkInfo, args) {
    const paraIdStr = args[0];
    const para = networkInfo.paras[paraIdStr];
    const relayNode = networkInfo.relay[0];

    await zombie.registerParachain(parseInt(paraIdStr,10),para.wasmPath, para.statePath, relayNode.wsUri, "//Alice", true);
}

module.exports = { run }
