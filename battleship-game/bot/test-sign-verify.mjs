import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  entropyToMiniSecret,
  mnemonicToEntropy,
  sr25519,
} from "@polkadot-labs/hdkd-helpers";
import {
  sr25519_sign,
  sr25519_verify,
  sr25519_pubkey,
  sr25519_keypair_from_seed,
} from "@polkadot-labs/schnorrkel-wasm";

function toHex(bytes) {
  return "0x" + Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Use a fixed mnemonic for reproducibility
const mnemonic = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
const miniSecret = entropyToMiniSecret(mnemonicToEntropy(mnemonic));

console.log("miniSecret:", toHex(miniSecret));

// Create keypair the same way the bot does
const derive = sr25519CreateDerive(miniSecret);
const keyPair = derive("");

console.log("keyPair.publicKey:", toHex(keyPair.publicKey));

// Also derive the public key from the raw keypair
const rawKeypair = sr25519_keypair_from_seed(miniSecret);
const rawSecret = rawKeypair.slice(0, 64);
const rawPubkey = rawKeypair.slice(64);
const derivedPubkey = sr25519_pubkey(rawSecret);

console.log("rawPubkey (from keypair):", toHex(rawPubkey));
console.log("derivedPubkey (from sr25519_pubkey):", toHex(derivedPubkey));
console.log("pubkeys match:", toHex(rawPubkey) === toHex(derivedPubkey));
console.log("keyPair.publicKey matches rawPubkey:", toHex(keyPair.publicKey) === toHex(rawPubkey));

// Sign a test message
const message = new TextEncoder().encode("test message");
const signature = keyPair.sign(message);
console.log("\nmessage:", toHex(message));
console.log("signature:", toHex(signature));
console.log("signature length:", signature.length);

// Verify with the schnorrkel-wasm verify function
const verified = sr25519_verify(keyPair.publicKey, message, signature);
console.log("sr25519_verify result:", verified);

// Now test with a proper statement-like payload
// Simulate encodeStatementForSigning
import { compact } from "scale-ts";
import { blake2b } from "@noble/hashes/blake2b";

const GAME_LOBBY_TOPIC = blake2b("battleship:lobby:v1", { dkLen: 32 });
const channel = blake2b(new TextEncoder().encode("battleship:creator:test:12345"), { dkLen: 32 });
const data = new TextEncoder().encode(JSON.stringify({ type: "announce", creator: "test", timestamp: 12345 }));
const expirySeconds = Math.floor(Date.now() / 1000) + 300;
const priority = 100;

// Build signing payload exactly as JS does
const parts = [];

// Expiry field
const expiryData = new Uint8Array(9);
expiryData[0] = 2;
new DataView(expiryData.buffer).setUint32(1, priority, true);
new DataView(expiryData.buffer).setUint32(5, expirySeconds, true);
parts.push(expiryData);

// Channel
const channelData = new Uint8Array(33);
channelData[0] = 3;
channelData.set(channel, 1);
parts.push(channelData);

// Topic
const topicData = new Uint8Array(33);
topicData[0] = 4;
topicData.set(GAME_LOBBY_TOPIC, 1);
parts.push(topicData);

// Data
const lenEnc = compact.enc(data.length);
const dataField = new Uint8Array(1 + lenEnc.length + data.length);
dataField[0] = 8;
dataField.set(lenEnc, 1);
dataField.set(data, 1 + lenEnc.length);
parts.push(dataField);

const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
const signingPayload = new Uint8Array(totalLen);
let offset = 0;
for (const part of parts) {
  signingPayload.set(part, offset);
  offset += part.length;
}

console.log("\n--- Statement signing test ---");
console.log("signing payload hex:", toHex(signingPayload));
console.log("signing payload length:", signingPayload.length);

const statementSig = keyPair.sign(signingPayload);
console.log("statement signature:", toHex(statementSig));

// Verify statement signature
const statementVerified = sr25519_verify(keyPair.publicKey, signingPayload, statementSig);
console.log("statement signature verified (JS):", statementVerified);

// Now build the full encoded statement (with proof) and decode it
// to check what Substrate would see
const fullParts = [];

// Proof: discriminant(0) + Sr25519(0) + signature(64) + signer(32)
const proofData = new Uint8Array(1 + 1 + 64 + 32);
proofData[0] = 0;
proofData[1] = 0; // Sr25519
proofData.set(statementSig, 2);
proofData.set(keyPair.publicKey, 66);
fullParts.push(proofData);

// Same fields as above
fullParts.push(expiryData);
fullParts.push(channelData);
fullParts.push(topicData);
fullParts.push(dataField);

// Encode with length prefix
const numFields = fullParts.length;
const fullTotalLen = fullParts.reduce((sum, p) => sum + p.length, 0);
const lenPrefix = compact.enc(numFields);
const fullStatement = new Uint8Array(lenPrefix.length + fullTotalLen);
fullStatement.set(lenPrefix, 0);
let fullOffset = lenPrefix.length;
for (const part of fullParts) {
  fullStatement.set(part, fullOffset);
  fullOffset += part.length;
}

console.log("\n--- Full statement ---");
console.log("full statement hex:", toHex(fullStatement));
console.log("full statement length:", fullStatement.length);
console.log("num fields:", numFields);
console.log("length prefix:", toHex(lenPrefix));

// Also sign with the raw sr25519_sign directly for comparison
const rawSig = sr25519_sign(keyPair.publicKey, rawSecret, signingPayload);
console.log("\n--- Raw sr25519_sign comparison ---");
console.log("raw signature:", toHex(rawSig));
const rawVerified = sr25519_verify(keyPair.publicKey, signingPayload, rawSig);
console.log("raw signature verified:", rawVerified);
