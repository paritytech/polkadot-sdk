import { readFileSync } from "fs";
import { createWalletClient, defineChain, http, parseEther, publicActions } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { parseArgs } from "util";
import { FlipperAbi } from "./abi/Flipper";

const {
	values: { endowment, ["private-key"]: privateKey },
} = parseArgs({
	args: process.argv.slice(2),
	options: {
		["private-key"]: {
			type: "string",
			short: "k",
		},
		endowment: {
			type: "string",
			short: "e",
		},
	},
});

export function assert(condition: any, message: string): asserts condition {
	if (!condition) {
		throw new Error(message);
	}
}

const rpcUrl = "http://localhost:8545";

export const chain = defineChain({
	id: 420420421,
	name: "Asset Hub Westend",
	network: "asset-hub",
	nativeCurrency: {
		name: "Westie",
		symbol: "WST",
		decimals: 18,
	},
	rpcUrls: {
		default: {
			http: [rpcUrl],
		},
	},
	testnet: true,
});

const wallet = createWalletClient({
	transport: http(),
	chain,
});
const [account] = await wallet.getAddresses();
export const serverWalletClient = createWalletClient({
	account,
	transport: http(),
	chain,
});

export const walletClient = await (async () => {
	if (privateKey) {
		const account = privateKeyToAccount(`0x${privateKey}`);
		console.log(`Wallet address ${account.address}`);

		const wallet = createWalletClient({
			account,
			transport: http(),
			chain,
		});

		if (endowment) {
			await serverWalletClient.sendTransaction({
				to: account.address,
				value: parseEther(endowment),
			});
			console.log(`Endowed address ${account.address} with: ${endowment}`);
		}

		return wallet.extend(publicActions);
	} else {
		return serverWalletClient.extend(publicActions);
	}
})();

const hash = await walletClient.deployContract({
	abi: FlipperAbi,
	bytecode: `0x${Buffer.from(readFileSync("pvm/Flipper.polkavm")).toString("hex")}`,
});
const deployReceipt = await walletClient.waitForTransactionReceipt({ hash });
const contractAddress = deployReceipt.contractAddress;
console.log("Contract deployed:", contractAddress);
assert(contractAddress, "Contract address should be set");
