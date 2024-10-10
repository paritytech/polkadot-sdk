import {
	BrowserProvider,
	Contract,
	ContractFactory,
	JsonRpcSigner,
	parseEther,
	encodeRlp,
	AddressLike,
	getBytes,
	Eip1193Provider,
} from "ethers";

declare global {
	interface Window {
		ethereum?: Eip1193Provider;
	}
}

document.addEventListener("DOMContentLoaded", async () => {
	if (typeof window.ethereum == "undefined") {
		return console.log("MetaMask is not installed");
	}

	console.log("MetaMask is installed!");
	const provider = new BrowserProvider(window.ethereum);

	console.log("Getting signer...");
	let signer: JsonRpcSigner;
	try {
		signer = await provider.getSigner();
		console.log(`Signer: ${signer.address}`);
	} catch (e) {
		console.error("Failed to get signer", e);
		return;
	}

	console.log("Getting block number...");
	try {
		const blockNumber = await provider.getBlockNumber();
		console.log(`Block number: ${blockNumber}`);
	} catch (e) {
		console.error("Failed to get block number", e);
		return;
	}

	const nonce = await signer.getNonce();
	console.log(`Nonce: ${nonce}`);

	document.getElementById("transferButton")?.addEventListener("click", async () => {
		const address = (document.getElementById("transferInput") as HTMLInputElement).value;
		await transfer(address);
	});

	document.getElementById("deployButton")?.addEventListener("click", async () => {
		await deploy();
	});
	document.getElementById("deployAndCallButton")?.addEventListener("click", async () => {
		const nonce = await signer.getNonce();
		console.log(`deploy with nonce: ${nonce}`);

		const address = await deploy();
		if (address) {
			const nonce = await signer.getNonce();
			console.log(`call with nonce: ${nonce}`);
			await call(address);
		}
	});
	document.getElementById("callButton")?.addEventListener("click", async () => {
		const address = (document.getElementById("callInput") as HTMLInputElement).value;
		await call(address);
	});

	async function deploy() {
		console.log("Deploying contract...");

		const code = getBytes(
			"0x50564d0001010424009000022363616c6cdeadbeef63616c6c5f6e657665726465706c6f797365616c5f72657475726e041001000000007365616c5f72657475726e051b03000a63616c6c5f6e6576657204066465706c6f79060463616c6c062c06011f000406081b1c06100408130013000211fc03100408040001040904040706100a05004e13005129a4b800",
		);
		const args = new Uint8Array();
		const bytecode = encodeRlp([code, args]);

		const contractFactory = new ContractFactory([], bytecode, signer);

		try {
			const contract = await contractFactory.deploy();
			await contract.waitForDeployment();
			const address = await contract.getAddress();
			console.log(`Contract deployed: ${address}`);
			return address;
		} catch (e) {
			console.error("Failed to deploy contract", e);
			return;
		}
	}

	async function call(address: string) {
		const abi = [
			"function getValue() view returns (uint256)",
			"function setValue(uint256 _value)",
		];

		const contract = new Contract(address, abi, signer);
		const tx = await contract.setValue(42);

		console.log("Transaction hash:", tx.hash);
	}

	async function transfer(to: AddressLike) {
		console.log(`transferring 1 DOT to ${to}...`);
		try {
			const tx = await signer.sendTransaction({
				to,
				value: parseEther("1.0"),
			});

			const receipt = await tx.wait();
			console.log(`Transaction hash: ${receipt?.hash}`);
		} catch (e) {
			console.error("Failed to send transaction", e);
			return;
		}
	}
});
