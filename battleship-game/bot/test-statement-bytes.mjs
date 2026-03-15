// Generate a full statement with known values for Rust cross-verification
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  entropyToMiniSecret,
  mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";
import {
  sr25519_sign,
  sr25519_verify,
  sr25519_pubkey,
  sr25519_keypair_from_seed,
} from "@polkadot-labs/schnorrkel-wasm";
import { compact } from "scale-ts";

function toHex(bytes) {
  return Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Use all-zeros seed for reproducibility
const seed = new Uint8Array(32);
const keypair = sr25519_keypair_from_seed(seed);
const secret = keypair.slice(0, 64);
const pubkey = keypair.slice(64);

// Use fixed, simple values
const expirySeconds = 1000;  // simple number
const priority = 42;
const channel = new Uint8Array(32); channel[0] = 0xCC; // simple channel
const topic = new Uint8Array(32); topic[0] = 0xAA; // simple topic
const data = new TextEncoder().encode("test data");

// Build signing payload (same as encodeStatementForSigning)
const parts = [];

// Expiry field: 0x02 + u64 LE
const expiryData = new Uint8Array(9);
expiryData[0] = 2;
new DataView(expiryData.buffer).setUint32(1, priority, true); // lower 32 bits
new DataView(expiryData.buffer).setUint32(5, expirySeconds, true); // upper 32 bits
parts.push(expiryData);

// Channel: 0x03 + 32 bytes
const channelData = new Uint8Array(33);
channelData[0] = 3;
channelData.set(channel, 1);
parts.push(channelData);

// Topic1: 0x04 + 32 bytes
const topicData = new Uint8Array(33);
topicData[0] = 4;
topicData.set(topic, 1);
parts.push(topicData);

// Data: 0x08 + compact(len) + raw bytes
const lenEnc = compact.enc(data.length);
const dataField = new Uint8Array(1 + lenEnc.length + data.length);
dataField[0] = 8;
dataField.set(lenEnc, 1);
dataField.set(data, 1 + lenEnc.length);
parts.push(dataField);

// Concatenate signing payload
const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
const signingPayload = new Uint8Array(totalLen);
let offset = 0;
for (const part of parts) {
  signingPayload.set(part, offset);
  offset += part.length;
}

console.log("=== Signing Payload ===");
console.log("signing_payload:", toHex(signingPayload));
console.log("signing_payload length:", signingPayload.length);

// Sign
const signature = sr25519_sign(pubkey, secret, signingPayload);
console.log("signature:", toHex(signature));

// Verify in JS
console.log("JS verify:", sr25519_verify(pubkey, signingPayload, signature));

// Build full statement with proof
const fullParts = [];

// Proof: 0x00 + 0x00 (Sr25519) + sig(64) + signer(32)
const proofData = new Uint8Array(1 + 1 + 64 + 32);
proofData[0] = 0;
proofData[1] = 0;
proofData.set(signature, 2);
proofData.set(pubkey, 66);
fullParts.push(proofData);

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

console.log("\n=== Full Statement ===");
console.log("full_statement:", toHex(fullStatement));
console.log("full_statement length:", fullStatement.length);

// Now let's also print what Rust would compute as signature_material
// Rust's encoded(for_signing=true) does:
//   [2u8][expiry u64 LE 8 bytes] = same as our expiryData
//   [3u8][channel 32 bytes] = same as our channelData  
//   [4u8][topic 32 bytes] = same as our topicData
//   [8u8][SCALE Vec<u8> of data] = [8u8][compact(len)][raw bytes]
// So the signing payloads should be identical.

// Let's break down the expiry u64 value:
const expiryU64 = BigInt(expirySeconds) * BigInt(2**32) + BigInt(priority);
console.log("\n=== Values for Rust ===");
console.log("expiry u64:", expiryU64.toString());
console.log("expiry_seconds:", expirySeconds);
console.log("priority:", priority);
console.log("pubkey:", toHex(pubkey));
console.log("channel:", toHex(channel));
console.log("topic:", toHex(topic));
console.log("data:", toHex(data));
console.log("data as string:", new TextDecoder().decode(data));

// Print Rust test
console.log(`
\n=== Rust test ===
#[test]
fn cross_verify_js_statement() {
    let pair = sr25519::Pair::from_seed(&[0u8; 32]);

    // Build statement the way Rust would
    let mut statement = Statement::new();
    statement.set_expiry(${expiryU64}u64);
    statement.set_channel([0xCC, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    statement.set_topic(0, [0xAA, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into());
    statement.set_plain_data(b"test data".to_vec());

    // Get the signing material that Rust would produce
    let rust_signing_material = statement.signature_material();
    let js_signing_payload = hex_to_bytes("${toHex(signingPayload)}");
    assert_eq!(
        rust_signing_material, js_signing_payload,
        "Signing payloads don't match!\\nRust: {}\\nJS:   {}",
        rust_signing_material.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(""),
        js_signing_payload.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
    );

    // Now verify the JS-produced signature
    let js_sig_bytes = hex_to_bytes("${toHex(signature)}");
    let js_sig = sr25519::Signature::from_raw(js_sig_bytes.as_slice().try_into().unwrap());
    let verified = sr25519::Pair::verify(&js_sig, &rust_signing_material[..], &pair.public());
    assert!(verified, "JS statement signature should verify in Rust!");

    // Also decode the full statement and verify
    let full_statement_bytes = hex_to_bytes("${toHex(fullStatement)}");
    let decoded = Statement::decode(&mut &full_statement_bytes[..]).unwrap();
    let result = decoded.verify_signature();
    assert!(
        matches!(result, SignatureVerificationResult::Valid(_)),
        "Full statement should verify! Got: {:?}", result
    );
}
`);
