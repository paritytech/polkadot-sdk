//! Run with bun run script.ts

import { readFileSync } from "fs";
import { compile } from "@parity/revive";
import { Contract, ContractFactory, JsonRpcProvider, TransactionResponse } from "ethers";
import { InterfaceAbi } from "ethers";

const provider = new JsonRpcProvider("http://localhost:8545");
const signer = await provider.getSigner();
console.log(`Signer address: ${await signer.getAddress()}, Nonce: ${await signer.getNonce()}`);

// deploy
async function deploy(bytecode: string, abi: InterfaceAbi) {
	console.log(`Deploying Contract...`);

	const contractFactory = new ContractFactory(abi, bytecode, signer);

	console.log("Deploying contract");
	const contract = await contractFactory.deploy();
	await contract.waitForDeployment();
	const address = await contract.getAddress();
	console.log(`Contract deployed: ${address}`);
	return address;
}

async function call(address: string, abi: InterfaceAbi) {
	console.log(`Calling Contract at ${address}...`);
	const contract = new Contract(address, abi, signer);
	const tx = (await contract.do_revert()) as TransactionResponse;
	console.log("Call transaction hash:", tx.hash);
	tx.wait();
}

try {
	const res = await compile({
		["revert.sol"]: { content: readFileSync("./contracts/Revert.sol", "utf8") },
	});

	const out = res.contracts["revert.sol"]["RevertExample"];
	const {
		abi,
		evm: {
			bytecode: { object: bytecode },
		},
	} = out;

	const address = await deploy(bytecode, abi);
	await call(address, abi);
} catch (err) {
	console.error(err);
}
