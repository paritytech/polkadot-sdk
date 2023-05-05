async function run(nodeName, networkInfo, args) {
    const paraIdStr = args[0];
    const para = networkInfo.paras[paraIdStr];
    const relayNode = networkInfo.relay[0];

    const registerParachainOptions = {
        id: parseInt(paraIdStr,10),
        wasmPath: para.wasmPath,
        statePath: para.statePath,
        apiUrl: relayNode.wsUri,
        onboardAsParachain: true,
        seed: "//Alice",
        finalization: true
    };


    await zombie.registerParachain(registerParachainOptions);
}

module.exports = { run }
