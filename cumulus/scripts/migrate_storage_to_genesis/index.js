const fs = require('fs-extra');
const yargs = require('yargs');
const { ApiPromise, WsProvider } = require('@polkadot/api');
var { xxhashAsHex } = require('@polkadot/util-crypto');

const openFile = (filePath) => {
	try {
		const jsonData = fs.readJSONSync(filePath);
		return jsonData;
	} catch(e) {
		console.log(`${filePath} file could not be opened. Make sure it exists`);
		process.exit(1);
	}
}

const connect = async () => {
	const wsProvider = new WsProvider('wss://rpc.polkadot.io');
	return await ApiPromise.create({ provider: wsProvider });
}

const queryStorage = async (api, argv) => {
	let rootKey = argv.key ? argv.key : xxhashAsHex(argv.pallet);

	let storageKeyValues = new Object();

	let keys = await api.rpc.state.getKeys(rootKey);

	console.log("Querying pallet state...")

	for (let key of keys) {
		let value = await api.rpc.state.getStorage(key);
		storageKeyValues[key] = value
	}

	return storageKeyValues;
}

const editChainSpec = (filePath, jsonData, storageKeyValues) => {
	console.log("Editing json chain_spec...")

	if (jsonData.genesis?.raw?.top) {
		for (const key in storageKeyValues) {
			jsonData.genesis.raw.top[key] = storageKeyValues[key];
		}
	}

	// Write the modified JSON data back to the file
	fs.writeJSONSync(filePath, jsonData, { spaces: 2 });
}

const run = async () => {
	const argv = yargs
	.option('file', {
		alias: 'f',
		describe: 'Path to the JSON file',
		demandOption: true, // Requires the --file option
		type: 'string',
	})
	.option('pallet', {
		alias: 'p',
		describe: 'Pallet name',
		type: 'string',
	})
	.option('key', {
		alias: 'k',
		describe: 'Storage key',
		type: 'string',
	})
	.check((args) => {
		if (!args.pallet && !args.key) {
			throw new Error('Please provide either --pallet or --key option.');
		}
		return true;
	})
	.help()
	.alias('help', 'h').argv;

	const filePath = argv.file;

	let jsonData = openFile(filePath);

	let api = await connect();

	let storageKeyValues = await queryStorage(api, argv);

	editChainSpec(filePath, jsonData, storageKeyValues)

	process.exit(0);
}

run()
