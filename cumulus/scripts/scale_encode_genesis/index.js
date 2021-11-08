const fs = require("fs");
const { exit } = require("process");
const {WsProvider, ApiPromise} = require("@polkadot/api");
const util = require("@polkadot/util");

async function connect(port, types) {
	const provider = new WsProvider("ws://127.0.0.1:" + port);
	const api = await ApiPromise.create({
		provider,
		types,
		throwOnConnect: false,
	});
	return api;
}

if (!process.argv[2] || !process.argv[3]) {
  console.log("usage: node generate_keys <input json> <scale output file>");
  exit();
}

const input = process.argv[2];
const output = process.argv[3];
fs.readFile(input, "utf8", (err, data) => {
  if (err) {
    console.log(`Error reading file from disk: ${err}`);
    exit(1);
  }

  const genesis = JSON.parse(data);

  connect(9944, {}).then(api => {
	const setStorage = api.tx.system.setStorage(genesis);
	const raw = setStorage.method.toU8a();
	const hex = util.u8aToHex(raw);
	fs.writeFileSync(output, hex);
	exit(0)
  }).catch(e => {
	  console.error(e);
	  exit(1)
  });
});
