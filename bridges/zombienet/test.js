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
	const api = await connect("ws://127.0.0.1:9910");

	const accountAddress = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
	const accountData = await api.query.system.account(accountAddress);
	console.log(accountData.data['free']);
	console.log(accountData.data['free'] > 0);
	console.log(accountData.data['free'] < 0xFFFF);
}

test()