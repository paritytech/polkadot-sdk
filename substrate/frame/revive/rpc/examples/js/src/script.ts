//! Run with one of
// bun run script.ts
// deno run --allow-all script.ts
import {
  Contract,
  ContractFactory,
  encodeRlp,
  getBytes,
  JsonRpcProvider,
} from "ethers";

const provider = new JsonRpcProvider("http://localhost:9090");
const signer = await provider.getSigner();
console.log(
  `Signer address: ${await signer.getAddress()}, Nonce: ${await signer
    .getNonce()}`,
);

function str_to_bytes(str: string): Uint8Array {
  return new TextEncoder().encode(str);
}

// deploy
async function deploy() {
  console.log(`Deploying Contract...`);
  const code = getBytes(
    "0x50564d000101041c009000021c6465706c6f7963616c6c696e7075746465706f7369745f6576656e74041b02000000000d0000006465706f7369745f6576656e74696e707574050f0203066465706c6f79560463616c6c0681661b02810a0000030020002f003a00470055005600720081008c009a00a800a900af00ba00bd00c500d100e300eb00f100f300f800fc00040106014e1300021174ff03108800031584005217040980000405800004080610068e0003158000521702188000061008dc00011a800004078100297a1e04070000015219040806100cbd011088000115840002118c00130000021174ff03108800031584005217040980000405800004080610123b031580005217021880000610148a00011a800004078100297a1f0407000001521904080610186bff011088000115840002118c00130000040a102fa947287a12aa0308a70b070a0e527c1110c802cc012fbcfb14a909129cfc08cb0a2e0c1d128cff0004020000010102220101222c0c1103bc02bb042fabfb1299030f090a0513527a07090f089a091110a802aa012f9afb13004e0113008b88220a518488848a8868441451441122122a44449392b4244922882c492a5952fd00",
  );

  const args = str_to_bytes("hello");
  const bytecode = encodeRlp([code, new Uint8Array()]);

  const contractFactory = new ContractFactory(
    [
      "constructor(bytes memory _data)",
    ],
    bytecode,
    signer,
  );

  console.log("Deploying contract with args:", args);
  const contract = await contractFactory.deploy(args);
  await contract.waitForDeployment();
  const address = await contract.getAddress();
  console.log(`Contract deployed: ${address}`);
  return address;
}

async function call(address: string) {
  console.log(`Calling Contract at ${address}...`);

  const abi = ["function call(bytes data)"];
  const contract = new Contract(address, abi, signer);
  const tx = await contract.call(str_to_bytes("world"));
  console.log("Call transaction hash:", tx.hash);
}

const address = await deploy();
await call(address);
