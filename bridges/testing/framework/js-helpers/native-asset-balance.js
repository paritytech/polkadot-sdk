async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const accountAddress = args[0];
    const accountData = await api.query.system.account(accountAddress);
    const accountBalance = accountData.data['free'];
    console.log("Balance of " + accountAddress + ": " + accountBalance);
    return accountBalance;
}

module.exports = {run}
