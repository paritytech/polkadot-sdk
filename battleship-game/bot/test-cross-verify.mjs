// Cross-verification test: produce values for Rust test
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  entropyToMiniSecret,
  mnemonicToEntropy,
  DEV_PHRASE,
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

// Use Alice's well-known seed (all zeros)
const seed = new Uint8Array(32); // 32 zero bytes - "Alice" seed
const keypair = sr25519_keypair_from_seed(seed);
const secret = keypair.slice(0, 64);
const pubkey = keypair.slice(64);

console.log("=== Cross-verification test values ===");
console.log("seed (32 bytes):", toHex(seed));
console.log("secret (64 bytes):", toHex(secret));
console.log("pubkey (32 bytes):", toHex(pubkey));

// Sign a simple known message
const message = new TextEncoder().encode("hello");
const sig = sr25519_sign(pubkey, secret, message);

console.log("message:", toHex(message));
console.log("signature:", toHex(sig));

// Verify in JS
console.log("JS verify:", sr25519_verify(pubkey, message, sig));

// Now sign a simple byte array [1, 2, 3]
const message2 = new Uint8Array([1, 2, 3]);
const sig2 = sr25519_sign(pubkey, secret, message2);
console.log("\nmessage2:", toHex(message2));
console.log("signature2:", toHex(sig2));
console.log("JS verify2:", sr25519_verify(pubkey, message2, sig2));

// Also try with the DEV_PHRASE mnemonic which is Alice
const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
console.log("\n=== DEV_PHRASE (Alice) ===");
console.log("miniSecret:", toHex(miniSecret));

const devKeypair = sr25519_keypair_from_seed(miniSecret);
const devSecret = devKeypair.slice(0, 64);
const devPubkey = devKeypair.slice(64);

console.log("pubkey:", toHex(devPubkey));

const sig3 = sr25519_sign(devPubkey, devSecret, message);
console.log("signature of 'hello':", toHex(sig3));
console.log("JS verify:", sr25519_verify(devPubkey, message, sig3));

// Print Rust test code
console.log("\n=== Rust test code ===");
console.log(`
// In sp-statement-store/src/lib.rs or a new test file
#[test]
fn cross_verify_js_signature() {
    use sp_core::sr25519;
    use sp_core::Pair;

    // All-zeros seed
    let seed = [0u8; 32];
    let pair = sr25519::Pair::from_seed_slice(&seed).unwrap();
    let public = pair.public();
    println!("Rust pubkey: {:?}", array_bytes::bytes2hex("", public.as_ref()));

    // JS pubkey for comparison
    let js_pubkey_hex = "${Array.from(pubkey).map(b => b.toString(16).padStart(2, '0')).join('')}";
    println!("JS pubkey:   {}", js_pubkey_hex);

    // Verify the public keys match
    assert_eq!(
        array_bytes::bytes2hex("", public.as_ref()),
        js_pubkey_hex,
        "Public keys don't match!"
    );

    // JS signature of "hello"
    let js_sig_bytes: [u8; 64] = array_bytes::hex2array_unchecked("${Array.from(sig).map(b => b.toString(16).padStart(2, '0')).join('')}");
    let js_sig = sr25519::Signature::from_raw(js_sig_bytes);

    let message = b"hello";
    let verified = sr25519::Pair::verify(&js_sig, &message[..], &public);
    println!("Rust verify JS sig: {}", verified);
    assert!(verified, "JS signature should verify in Rust!");
}
`);
