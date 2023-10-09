const fs = require('fs-extra');
const yargs = require('yargs');
const { ApiPromise, WsProvider } = require('@polkadot/api');
const { hexToString } = require('@polkadot/util');
var { xxhashAsHex } = require('@polkadot/util-crypto');


// Open chain_spec raw file
const openFile = (filePath) => {
	try {
		const jsonData = fs.readJSONSync(filePath);
		return jsonData;
	} catch(e) {
		console.log(`${filePath} file could not be opened. Make sure it exists`);
		process.exit(1);
	}
}

// Connect to chain
const connect = async (endpoint) => {
	console.log(`Conecting to ${endpoint}...`);
	const wsProvider = new WsProvider(endpoint);
	return await ApiPromise.create({ provider: wsProvider });
}

const queryIdentityOf = async (api) => {
	let allEntries = await api.query.identity.identityOf.entries();

	let totalIdentities = 0;
	let identitiesWithAdditional = 0;
	let additionalKeys = {}

	allEntries.forEach(([a, b]) => {
		JSON.parse(b).info.additional.forEach(([key, value]) => {
			identitiesWithAdditional += 1;

			let keyString = hexToString(key.raw)
			let valueString = hexToString(value.raw)

			// console.log(keyString, valueString)

			if (additionalKeys[keyString] !== undefined) {
				additionalKeys[keyString] += 1
			} else {
				additionalKeys[keyString] = 1
			}
		});
		totalIdentities += 1;
	});

	console.log(additionalKeys)
	console.log("Total", totalIdentities);
	console.log("With additional", identitiesWithAdditional);
}

// Query all storage under a certain key
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

// Edit chain_spec raw adding the queried storage
const editChainSpec = (filePath, jsonData, storageKeyValues) => {
	console.log("Editing json chain_spec...")

	if (jsonData.genesis?.raw?.top) {
		for (const key in storageKeyValues) {
			jsonData.genesis.raw.top[key] = storageKeyValues[key];
		}
	} else {
		console.log(`${filePath} - invalid raw chain_spec format json file`);
		process.exit(1);
	}

	fs.writeJSONSync(filePath, jsonData, { spaces: 2 });
}

const run = async () => {
	const argv = yargs
	.option('chain', {
		alias: 'c',
		describe: 'Endpoint to the chain to query sorage',
		demandOption: true, // Requires the --file option
		type: 'string',
	})
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

	let api = await connect(argv.chain);

	// queryIdentityOf(api);

	let storageKeyValues = await queryStorage(api, argv);

	editChainSpec(filePath, jsonData, storageKeyValues)

	process.exit(0);
}

run()
