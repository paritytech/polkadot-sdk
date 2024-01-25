#!node
const { WsProvider, ApiPromise } = require("@polkadot/api");
const util = require("@polkadot/util");

async function connect(endpoint, types = {}) {
	const provider = new WsProvider(endpoint);
	const api = await ApiPromise.create({
		provider,
		types: {/*
			HeaderId: {
				number: "u32",
				hash: "H256"
			}*/
		},
		throwOnConnect: false,
	});
	return api;
}

async function test() {
	const api = await connect("wss://rococo-bridge-hub-rpc.polkadot.io");

	let count = 0;
	console.log("begin");
	const unsubscribe = await api.rpc.chain.subscribeNewHeads((header) => {
		++count;
		console.log(count);
		return 1;
		if (count === 2) {
			console.log('2 headers retrieved, unsubscribing');
			unsubscribe();
			resolve(123)
		}
	}).then((v) => { console.log(100); });
	console.log("end");
	//await new Promise(resolve => setTimeout(resolve, 60 * 1000));
	console.log(unsubscribe);
	console.log("end++");
}

test()