const fs = require("fs");
const { exit } = require("process");
const { WsProvider, ApiPromise } = require("@polkadot/api");
const util = require("@polkadot/util");

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

function writeHexEncodedBytesToOutput(method, outputFile) {
	console.log("Payload (hex): ", method.toHex());
	console.log("Payload (bytes): ", Array.from(method.toU8a()));
	console.log("Payload (plain): ", JSON.stringify(method));
	fs.writeFileSync(outputFile, JSON.stringify(Array.from(method.toU8a())));
}

function remarkWithEvent(endpoint, outputFile) {
	console.log(`Generating remarkWithEvent from RPC endpoint: ${endpoint} to outputFile: ${outputFile}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.system.remarkWithEvent("Hello");
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function addExporterConfig(endpoint, outputFile, bridgedNetwork, bridgeConfig) {
	console.log(`Generating addExporterConfig from RPC endpoint: ${endpoint} to outputFile: ${outputFile} based on bridgedNetwork: ${bridgedNetwork}, bridgeConfig: ${bridgeConfig}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.bridgeTransfer.addExporterConfig(bridgedNetwork, JSON.parse(bridgeConfig));
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function addUniversalAlias(endpoint, outputFile, location, junction) {
	console.log(`Generating addUniversalAlias from RPC endpoint: ${endpoint} to outputFile: ${outputFile} based on location: ${location}, junction: ${junction}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.bridgeTransfer.addUniversalAlias(JSON.parse(location), JSON.parse(junction));
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function addReserveLocation(endpoint, outputFile, reserve_location) {
	console.log(`Generating addReserveLocation from RPC endpoint: ${endpoint} to outputFile: ${outputFile} based on reserve_location: ${reserve_location}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.bridgeTransfer.addReserveLocation(JSON.parse(reserve_location));
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function removeExporterConfig(endpoint, outputFile, bridgedNetwork) {
	console.log(`Generating removeExporterConfig from RPC endpoint: ${endpoint} to outputFile: ${outputFile} based on bridgedNetwork: ${bridgedNetwork}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.bridgeTransfer.removeExporterConfig(bridgedNetwork);
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function forceCreateAsset(endpoint, outputFile, assetId, assetOwnerAccountId, isSufficient, minBalance) {
	var isSufficient = isSufficient == "true" ? true : false;
	console.log(`Generating forceCreateAsset from RPC endpoint: ${endpoint} to outputFile: ${outputFile} based on assetId: ${assetId}, assetOwnerAccountId: ${assetOwnerAccountId}, isSufficient: ${isSufficient}, minBalance: ${minBalance}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.foreignAssets.forceCreate(JSON.parse(assetId), assetOwnerAccountId, isSufficient, minBalance);
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

function forceXcmVersion(endpoint, outputFile, dest, xcm_version) {
	console.log(`Generating forceXcmVersion from RPC endpoint: ${endpoint} to outputFile: ${outputFile}, dest: ${dest}, xcm_version: ${xcm_version}`);
	connect(endpoint)
		.then((api) => {
			const call = api.tx.polkadotXcm.forceXcmVersion(JSON.parse(dest), xcm_version);
			writeHexEncodedBytesToOutput(call.method, outputFile);
			exit(0);
		})
		.catch((e) => {
			console.error(e);
			exit(1);
		});
}

if (!process.argv[2] || !process.argv[3]) {
	console.log("usage: node ./script/generate_hex_encoded_call <type> <endpoint> <output hex-encoded data file> <input message>");
	exit(1);
}

const type = process.argv[2];
const rpcEndpoint = process.argv[3];
const output = process.argv[4];
const inputArgs = process.argv.slice(5, process.argv.length);
console.log(`Generating hex-encoded call data for:`);
console.log(`	type: ${type}`);
console.log(`	rpcEndpoint: ${rpcEndpoint}`);
console.log(`	output: ${output}`);
console.log(`	inputArgs: ${inputArgs}`);

switch (type) {
	case 'remark-with-event':
		remarkWithEvent(rpcEndpoint, output);
		break;
	case 'add-exporter-config':
		addExporterConfig(rpcEndpoint, output, inputArgs[0], inputArgs[1]);
		break;
	case 'remove-exporter-config':
		removeExporterConfig(rpcEndpoint, output, inputArgs[0], inputArgs[1]);
		break;
	case 'add-universal-alias':
		addUniversalAlias(rpcEndpoint, output, inputArgs[0], inputArgs[1]);
		break;
	case 'add-reserve-location':
		addReserveLocation(rpcEndpoint, output, inputArgs[0]);
		break;
	case 'force-create-asset':
		forceCreateAsset(rpcEndpoint, output, inputArgs[0], inputArgs[1], inputArgs[2], inputArgs[3]);
		break;
	case 'force-xcm-version':
		forceXcmVersion(rpcEndpoint, output, inputArgs[0], inputArgs[1]);
		break;
	case 'check':
		console.log(`Checking nodejs installation, if you see this everything is ready!`);
		break;
	default:
		console.log(`Sorry, we are out of ${type} - not yet supported!`);
}
