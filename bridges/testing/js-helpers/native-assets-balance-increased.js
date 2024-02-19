async function run(nodeName, networkInfo, args) {
    const {wsUri, userDefinedTypes} = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    const accountAddress = args[0];
    const initialAccountData = await api.query.system.account(accountAddress);
    const initialAccountBalance = initialAccountData.data['free'];
    while (true) {
        const accountData = await api.query.system.account(accountAddress);
        const accountBalance = accountData.data['free'];
        if (accountBalance > initialAccountBalance) {
            return accountBalance;
        }

        // else sleep and retry
        await new Promise((resolve) => setTimeout(resolve, 6000));
    }
}

module.exports = { run }
