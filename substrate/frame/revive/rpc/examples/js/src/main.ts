import {
  AddressLike,
  BrowserProvider,
  Contract,
  ContractFactory,
  Eip1193Provider,
  JsonRpcSigner,
  parseEther,
} from "ethers";

declare global {
  interface Window {
    ethereum?: Eip1193Provider;
  }
}

function str_to_bytes(str: string): Uint8Array {
  return new TextEncoder().encode(str);
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

  document.getElementById("transferButton")?.addEventListener(
    "click",
    async () => {
      const address =
        (document.getElementById("transferInput") as HTMLInputElement).value;
      await transfer(address);
    },
  );

  document.getElementById("deployButton")?.addEventListener(
    "click",
    async () => {
      await deploy();
    },
  );
  document.getElementById("deployAndCallButton")?.addEventListener(
    "click",
    async () => {
      const nonce = await signer.getNonce();
      console.log(`deploy with nonce: ${nonce}`);

      const address = await deploy();
      if (address) {
        const nonce = await signer.getNonce();
        console.log(`call with nonce: ${nonce}`);
        await call(address);
      }
    },
  );
  document.getElementById("callButton")?.addEventListener("click", async () => {
    const address =
      (document.getElementById("callInput") as HTMLInputElement).value;
    await call(address);
  });

  async function deploy() {
    console.log("Deploying contract...");

    const bytecode = await fetch("rpc_demo.polkavm").then((response) => {
      if (!response.ok) {
        throw new Error("Network response was not ok");
      }
      return response.arrayBuffer();
    })
      .then((arrayBuffer) => new Uint8Array(arrayBuffer));

    const contractFactory = new ContractFactory(
      [
        "constructor(bytes memory _data)",
      ],
      bytecode,
      signer,
    );

    try {
      const args = str_to_bytes("hello");
      const contract = await contractFactory.deploy(args);
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
    const abi = ["function call(bytes data)"];
    const contract = new Contract(address, abi, signer);
    const tx = await contract.call(str_to_bytes("world"));

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
