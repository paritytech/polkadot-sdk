const fs = require("fs");
const { exit } = require("process");
const { WsProvider, ApiPromise } = require("@polkadot/api");
const util = require("@polkadot/util");

// Utility script constructing a SCALE-encoded setStorage call from a key-value json array of
// genesis values by connecting to a running instance of the chain. (It is not required to be
// functional or synced.)

// connect to a substrate chain and return the api object
async function connect(endpoint, types = {}) {
	const provider = new WsProvider(endpoint);
	const api = await ApiPromise.create({
		provider,
		types,
		throwOnConnect: false,
	});
	return api;
}

if (!process.argv[2] || !process.argv[3]) {
	console.log("usage: node generate_keys <input json> <scale output file> [rpc endpoint]");
	exit();
}

const input = process.argv[2];
const output = process.argv[3];
// default to localhost and the default Substrate port
const rpcEndpoint = process.argv[4] || "ws://localhost:9944";

console.log("Processing", input, output);
fs.readFile(input, "utf8", (err, data) => {
	if (err) {
		console.log(`Error reading file from disk: ${err}`);
		exit(1);
	}

	const genesis = JSON.parse(data);

	console.log("loaded genesis, length =  ", genesis.length);
	console.log(`Connecting to RPC endpoint: ${rpcEndpoint}`);
	connect(rpcEndpoint)
		.then((api) => {
			console.log('Connected');
			const setStorage = api.tx.system.setStorage(genesis);
			const raw = setStorage.method.toU8a();
			const hex = util.u8aToHex(raw);
			fs.writeFileSync(output, hex);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
});
