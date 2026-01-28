import { createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/node";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  DEV_PHRASE,
  entropyToMiniSecret,
  mnemonicToEntropy,
  sr25519,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { blake2b } from "@noble/hashes/blake2b";
import { compact } from "scale-ts";

const WS_URL = "ws://localhost:36647";

interface GameAnnouncement {
  creator: string;
  potAmount: string;
  timestamp: number;
  onChainGameId?: string;
}

interface JoinResponse {
  joiner: string;
  timestamp: number;
}

const GAME_LOBBY_TOPIC = blake2b("battleship:lobby:v1", { dkLen: 32 });

function creatorChannel(creator: string): Uint8Array {
  return blake2b(
    new TextEncoder().encode(`battleship:creator:${creator}`),
    { dkLen: 32 }
  );
}

function joinResponseTopic(
  creator: string,
  joiner: string,
  timestamp: number
): Uint8Array {
  return blake2b(
    new TextEncoder().encode(`battleship:join:${creator}:${joiner}:${timestamp}`),
    { dkLen: 32 }
  );
}

function joinResponseChannel(
  creator: string,
  joiner: string,
  timestamp: number
): Uint8Array {
  return blake2b(
    new TextEncoder().encode(`battleship:join-channel:${creator}:${joiner}:${timestamp}`),
    { dkLen: 32 }
  );
}

function toHex(bytes: Uint8Array): string {
  return "0x" + Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex: string): Uint8Array {
  const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
  const bytes = new Uint8Array(cleanHex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
  }
  return bytes;
}

function encodeStatementForSigning(
  priority: number,
  channel: Uint8Array,
  topics: Uint8Array[],
  data: Uint8Array
): Uint8Array {
  const parts: Uint8Array[] = [];

  const priorityData = new Uint8Array(5);
  priorityData[0] = 2;
  new DataView(priorityData.buffer).setUint32(1, priority, true);
  parts.push(priorityData);

  const channelData = new Uint8Array(33);
  channelData[0] = 3;
  channelData.set(channel, 1);
  parts.push(channelData);

  for (let i = 0; i < Math.min(topics.length, 4); i++) {
    const topicData = new Uint8Array(33);
    topicData[0] = 4 + i;
    topicData.set(topics[i], 1);
    parts.push(topicData);
  }

  const lenEnc = compact.enc(data.length);
  const dataField = new Uint8Array(1 + lenEnc.length + data.length);
  dataField[0] = 8;
  dataField.set(lenEnc, 1);
  dataField.set(data, 1 + lenEnc.length);
  parts.push(dataField);

  const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
  const result = new Uint8Array(totalLen);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

function encodeStatementWithProof(
  signature: Uint8Array,
  signer: Uint8Array,
  priority: number,
  channel: Uint8Array,
  topics: Uint8Array[],
  data: Uint8Array
): Uint8Array {
  const parts: Uint8Array[] = [];

  const proofData = new Uint8Array(1 + 1 + 64 + 32);
  proofData[0] = 0;
  proofData[1] = 0;
  proofData.set(signature, 2);
  proofData.set(signer, 66);
  parts.push(proofData);

  const priorityData = new Uint8Array(5);
  priorityData[0] = 2;
  new DataView(priorityData.buffer).setUint32(1, priority, true);
  parts.push(priorityData);

  const channelData = new Uint8Array(33);
  channelData[0] = 3;
  channelData.set(channel, 1);
  parts.push(channelData);

  for (let i = 0; i < Math.min(topics.length, 4); i++) {
    const topicData = new Uint8Array(33);
    topicData[0] = 4 + i;
    topicData.set(topics[i], 1);
    parts.push(topicData);
  }

  const lenEnc = compact.enc(data.length);
  const dataField = new Uint8Array(1 + lenEnc.length + data.length);
  dataField[0] = 8;
  dataField.set(lenEnc, 1);
  dataField.set(data, 1 + lenEnc.length);
  parts.push(dataField);

  const numFields = parts.length;
  const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
  const lenPrefix = compact.enc(numFields);
  const result = new Uint8Array(lenPrefix.length + totalLen);
  result.set(lenPrefix, 0);
  let offset = lenPrefix.length;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

function createSigner(path: string) {
  const entropy = mnemonicToEntropy(DEV_PHRASE);
  const miniSecret = entropyToMiniSecret(entropy);
  const derive = sr25519CreateDerive(miniSecret);
  const keyPair = derive(path);
  return getPolkadotSigner(keyPair.publicKey, "Sr25519", (input) =>
    keyPair.sign(input)
  );
}

async function test() {
  console.log("Connecting to chain...");
  const provider = getWsProvider(WS_URL);
  const client = createClient(withPolkadotSdkCompat(provider));

  const aliceSigner = createSigner("//Alice");
  const bobSigner = createSigner("//Bob");

  const aliceAddress = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
  const bobAddress = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";

  console.log("\n=== Test 1: Alice announces a game ===");
  const announcement: GameAnnouncement = {
    creator: aliceAddress,
    potAmount: "1000000000000",
    timestamp: Date.now(),
  };

  const data = new TextEncoder().encode(JSON.stringify(announcement));
  // Use timestamp in channel to avoid conflicts with previous test runs
  const channel = blake2b(
    new TextEncoder().encode(`battleship:creator:${aliceAddress}:${announcement.timestamp}`),
    { dkLen: 32 }
  );
  const priority = 100;

  const signingPayload = encodeStatementForSigning(priority, channel, [GAME_LOBBY_TOPIC], data);
  const signature = await aliceSigner.signBytes(signingPayload);

  const statement = encodeStatementWithProof(
    signature,
    aliceSigner.publicKey,
    priority,
    channel,
    [GAME_LOBBY_TOPIC],
    data
  );

  const submitResult = await (client as any)._request("statement_submit", [toHex(statement)]);
  console.log("Alice announce result:", submitResult);

  if (submitResult.status !== "broadcast" && submitResult.status !== "new") {
    console.error("FAIL: Expected broadcast/new status, got:", submitResult);
    process.exit(1);
  }
  console.log("PASS: Alice's game announced");

  console.log("\n=== Test 2: Bob sees Alice's game ===");
  const topicArray = Array.from(GAME_LOBBY_TOPIC);
  const broadcasts = await (client as any)._request("statement_broadcasts", [[topicArray]]);

  if (!broadcasts || broadcasts.length === 0) {
    console.error("FAIL: No games found in lobby");
    process.exit(1);
  }

  let foundGame: GameAnnouncement | null = null;
  for (const broadcastHex of broadcasts) {
    const dataBytes = fromHex(broadcastHex);
    const parsed = JSON.parse(new TextDecoder().decode(dataBytes)) as GameAnnouncement;
    if (parsed.creator === aliceAddress && parsed.timestamp === announcement.timestamp) {
      foundGame = parsed;
      break;
    }
  }

  if (!foundGame) {
    console.error("FAIL: Could not find Alice's game");
    process.exit(1);
  }
  console.log("PASS: Bob found Alice's game:", foundGame);

  console.log("\n=== Test 3: Bob sends join response ===");
  const joinResponse: JoinResponse = { joiner: bobAddress, timestamp: Date.now() };
  const joinData = new TextEncoder().encode(JSON.stringify(joinResponse));
  const joinTopic = joinResponseTopic(aliceAddress, bobAddress, announcement.timestamp);
  const joinChannel = joinResponseChannel(aliceAddress, bobAddress, announcement.timestamp);

  const joinSigningPayload = encodeStatementForSigning(100, joinChannel, [joinTopic], joinData);
  const joinSignature = await bobSigner.signBytes(joinSigningPayload);

  const joinStatement = encodeStatementWithProof(
    joinSignature,
    bobSigner.publicKey,
    100,
    joinChannel,
    [joinTopic],
    joinData
  );

  const joinSubmitResult = await (client as any)._request("statement_submit", [toHex(joinStatement)]);
  console.log("Bob join response result:", joinSubmitResult);

  if (joinSubmitResult.status !== "broadcast" && joinSubmitResult.status !== "new") {
    console.error("FAIL: Expected broadcast/new status for join, got:", joinSubmitResult);
    process.exit(1);
  }
  console.log("PASS: Bob's join response sent");

  console.log("\n=== Test 4: Alice sees Bob's join response ===");
  await new Promise((r) => setTimeout(r, 1000));

  const dumpResult = await (client as any)._request("statement_dump", []);
  let foundJoinResponse = false;

  for (const statementHex of dumpResult || []) {
    try {
      const statementBytes = fromHex(statementHex);
      const dataStart = statementBytes.lastIndexOf(0x08);
      if (dataStart === -1) continue;

      let offset = dataStart + 1;
      const firstByte = statementBytes[offset];
      let dataLen: number;
      if ((firstByte & 0b11) === 0b00) {
        dataLen = firstByte >> 2;
        offset += 1;
      } else if ((firstByte & 0b11) === 0b01) {
        dataLen = (statementBytes[offset] | (statementBytes[offset + 1] << 8)) >> 2;
        offset += 2;
      } else {
        continue;
      }

      const dataBytes = statementBytes.slice(offset, offset + dataLen);
      const jsonStr = new TextDecoder().decode(dataBytes);
      const parsed = JSON.parse(jsonStr);

      if (parsed.joiner === bobAddress) {
        foundJoinResponse = true;
        console.log("Found Bob's join response:", parsed);
        break;
      }
    } catch {
      continue;
    }
  }

  if (!foundJoinResponse) {
    console.error("FAIL: Alice could not find Bob's join response");
    process.exit(1);
  }
  console.log("PASS: Alice found Bob's join response");

  console.log("\n=== All tests passed! ===");
  client.destroy();
  process.exit(0);
}

test().catch((e) => {
  console.error("Test failed with error:", e);
  process.exit(1);
});
