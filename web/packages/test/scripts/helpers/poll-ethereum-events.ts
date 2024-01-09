
import Web3 from 'web3'
import { readFile } from 'fs/promises';

const main = async () => {
    var web3 = new Web3(new Web3.providers.WebsocketProvider('ws://127.0.0.1:8546'));

    const contracts = JSON.parse(await readFile(process.env["output_dir"] + '/contracts.json', 'utf8'));

    const interestingContracts = {
        WrappedToken: contracts.contracts.WrappedToken,
        BasicOutboundChannel: contracts.contracts.BasicOutboundChannel,
        BasicInboundChannel: contracts.contracts.BasicInboundChannel,
        BeefyClient: contracts.contracts.BeefyClient,
    };

    const instantiatedContracts: Array<any> = []
    console.log("Time", "BlockNumber", "TransactionIndex", "Contract", "LogIndex", "Event", "Data", "Error")
    for (const key of Object.keys(interestingContracts)) {
        const contract = interestingContracts[key];
        const ic = new web3.eth.Contract(contract.abi, contract.address);
        instantiatedContracts.push(ic);
        ic.events.allEvents({}, (error, event) => {
            console.log(new Date(), event.blockNumber, event.transactionIndex, event.logIndex, key, event.event, JSON.stringify(event.returnValues), JSON.stringify(error));
        }).on('error', function(error, receipt) {
            console.error(new Date(), "ERROR", key, receipt, JSON.stringify(error));
        });
    }

    process.on('SIGINT', () => process.exit(0));  // CTRL+C
};

main().catch((error) => {
    console.error(error);
    process.exit(1);
});
