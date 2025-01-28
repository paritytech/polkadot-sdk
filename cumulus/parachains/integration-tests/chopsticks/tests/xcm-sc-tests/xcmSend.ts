import { readFileSync } from "fs";
import { parseAbi } from "viem";
import { wnd_ah } from "@polkadot-api/descriptors";
import { createClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { account, assert, walletClient } from "./utils";

const ahClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:8000")));
const AHApi = ahClient.getTypedApi(wnd_ah);

const XcmExecuteAbi = parseAbi(["constructor()"]);
const hash = await walletClient.deployContract({
	abi: XcmExecuteAbi,
	bytecode: `0x${Buffer.from(readFileSync("pvm/XcmSend.polkavm")).toString("hex")}`,
});
const deployReceipt = await walletClient.waitForTransactionReceipt({ hash });
const rustContractAddress = deployReceipt.contractAddress;
console.log("Rust Contract deployed:", rustContractAddress);
assert(rustContractAddress, "Contract address should be set");

const rawXcmBytes = "0x05000005040601003448656c6c6f2c20576f726c6421"; // TODO: failing decoding

const estimatedGas = await walletClient.estimateGas({
	account,
	to: rustContractAddress,
	data: rawXcmBytes,
});
console.log("Gas:", Number(estimatedGas));
