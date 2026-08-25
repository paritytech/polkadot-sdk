// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Compile-coverage smoke test for the riscv (PolkaVM/JAM) native `sp_io` implementations.
//!
//! This module's sole purpose is to force the riscv compiler to type-check **every** function
//! re-exported from `sp_io`'s in-blob native module (`substrate/primitives/io/src/native/`).
//! On riscv only the storage host functions remain host calls; everything else is a native,
//! in-blob implementation with no test coverage anywhere else.
//!
//! Many of these natives intentionally `panic!` at runtime ("needs node-side state and has no
//! in-blob implementation" — keystore, offchain, tx-index, allocator, `misc::last_cursor`,
//! `misc::runtime_version`), and `panic_handler::abort_on_panic` diverges (`-> !`). Calling them
//! here is purely for type-checking; they are deliberately **never executed**. `touch_all` is
//! not wired into any runtime API, dispatchable, or `polkavm_export`ed entry point. Everything
//! panicking/diverging is reached behind `if false` so execution can never proceed past it.

use alloc::vec;

use sp_core::{
	crypto::KeyTypeId,
	ecdsa, ed25519,
	offchain::{HttpRequestId, StorageKind, Timestamp},
	sr25519,
	storage::StateVersion,
	OpaquePeerId, RuntimeInterfaceLogLevel, H256,
};
use sp_io::{Hash512, NetworkPeerId, Pubkey264, Pubkey512};
use sp_tracing::{WasmEntryAttributes, WasmMetadata};

/// Calls every `sp_io` native function with type-correct dummy arguments.
///
/// Never executed — compile coverage only.
pub fn touch_all() {
	touch_hashing();
	touch_trie();
	touch_crypto();
	touch_misc();
	touch_offchain();
	touch_offchain_index();
	touch_transaction_index();
	touch_allocator();
	touch_logging();
	touch_wasm_tracing();
	touch_panic_handler();
}

fn touch_hashing() {
	let data: &[u8] = b"smoke";
	let mut h16 = [0u8; 16];
	let mut h32 = [0u8; 32];
	let mut h8 = [0u8; 8];
	let mut h64 = Hash512::default();

	sp_io::hashing::blake2_128__raw(data, &mut h16);
	sp_io::hashing::blake2_256__raw(data, &mut h32);
	sp_io::hashing::keccak_256__raw(data, &mut h32);
	sp_io::hashing::keccak_512__raw(data, &mut h64);
	sp_io::hashing::sha2_256__raw(data, &mut h32);
	sp_io::hashing::twox_128__raw(data, &mut h16);
	sp_io::hashing::twox_256__raw(data, &mut h32);
	sp_io::hashing::twox_64__raw(data, &mut h8);

	let _ = sp_io::hashing::keccak_256(data);
	let _ = sp_io::hashing::keccak_512(data);
	let _ = sp_io::hashing::sha2_256(data);
	let _ = sp_io::hashing::blake2_128(data);
	let _ = sp_io::hashing::blake2_256(data);
	let _ = sp_io::hashing::twox_256(data);
	let _ = sp_io::hashing::twox_128(data);
	let _ = sp_io::hashing::twox_64(data);
}

fn touch_trie() {
	let mut root = H256::default();
	let version_v0 = StateVersion::V0;
	let version_v1 = StateVersion::V1;

	sp_io::trie::blake2_256_root__raw(vec![], version_v0, &mut root);
	sp_io::trie::blake2_256_ordered_root__raw(vec![], version_v0, &mut root);
	sp_io::trie::keccak_256_root__raw(vec![], version_v1, &mut root);
	sp_io::trie::keccak_256_ordered_root__raw(vec![], version_v1, &mut root);

	let _ = sp_io::trie::blake2_256_verify_proof(root, &[], &[], &[], version_v0);
	let _ = sp_io::trie::keccak_256_verify_proof(root, &[], &[], &[], version_v1);

	let _ = sp_io::trie::blake2_256_root(vec![], version_v0);
	let _ = sp_io::trie::blake2_256_ordered_root(vec![], version_v0);
	let _ = sp_io::trie::keccak_256_root(vec![], version_v1);
	let _ = sp_io::trie::keccak_256_ordered_root(vec![], version_v1);
}

fn touch_crypto() {
	let id = KeyTypeId([0u8; 4]);
	let mut ecdsa_pub = ecdsa::Public::default();
	let mut ecdsa_pubs = vec![ecdsa::Public::default()];
	let mut ecdsa_sig = ecdsa::Signature::default();
	let mut ed_pub = ed25519::Public::default();
	let mut ed_pubs = vec![ed25519::Public::default()];
	let mut ed_sig = ed25519::Signature::default();
	let mut sr_pub = sr25519::Public::default();
	let mut sr_pubs = vec![sr25519::Public::default()];
	let mut sr_sig = sr25519::Signature::default();
	let mut pub512 = Pubkey512::default();
	let mut pub264 = Pubkey264::default();
	let sig65 = [0u8; 65];
	let msg32 = [0u8; 32];

	sp_io::crypto::ecdsa_generate__raw(id, None, &mut ecdsa_pub);
	let _ = sp_io::crypto::ecdsa_public_keys__raw(id, &mut ecdsa_pubs);
	let _ = sp_io::crypto::ecdsa_sign__raw(id, &ecdsa_pub, b"msg", &mut ecdsa_sig);
	let _ = sp_io::crypto::ecdsa_sign_prehashed__raw(id, &ecdsa_pub, &msg32, &mut ecdsa_sig);
	let _ = sp_io::crypto::ecdsa_verify(&ecdsa_sig, b"msg", &ecdsa_pub);
	let _ = sp_io::crypto::ecdsa_verify_prehashed(&ecdsa_sig, &msg32, &ecdsa_pub);

	sp_io::crypto::ed25519_generate__raw(id, None, &mut ed_pub);
	let _ = sp_io::crypto::ed25519_public_keys__raw(id, &mut ed_pubs);
	let _ = sp_io::crypto::ed25519_sign__raw(id, &ed_pub, b"msg", &mut ed_sig);
	let _ = sp_io::crypto::ed25519_verify(&ed_sig, b"msg", &ed_pub);

	let _ = sp_io::crypto::secp256k1_ecdsa_recover__raw(&sig65, &msg32, &mut pub512);
	let _ = sp_io::crypto::secp256k1_ecdsa_recover_compressed__raw(&sig65, &msg32, &mut pub264);

	sp_io::crypto::sr25519_generate__raw(id, None, &mut sr_pub);
	let _ = sp_io::crypto::sr25519_public_keys__raw(id, &mut sr_pubs);
	let _ = sp_io::crypto::sr25519_sign__raw(id, &sr_pub, b"msg", &mut sr_sig);
	let _ = sp_io::crypto::sr25519_verify(&sr_sig, b"msg", &sr_pub);

	let _ = sp_io::crypto::ed25519_public_keys(id);
	let _ = sp_io::crypto::ed25519_generate(id, None);
	let _ = sp_io::crypto::ed25519_sign(id, &ed_pub, b"msg");
	let _ = sp_io::crypto::sr25519_public_keys(id);
	let _ = sp_io::crypto::sr25519_generate(id, None);
	let _ = sp_io::crypto::sr25519_sign(id, &sr_pub, b"msg");
	let _ = sp_io::crypto::ecdsa_public_keys(id);
	let _ = sp_io::crypto::ecdsa_generate(id, None);
	let _ = sp_io::crypto::ecdsa_sign(id, &ecdsa_pub, b"msg");
	let _ = sp_io::crypto::ecdsa_sign_prehashed(id, &ecdsa_pub, &msg32);
	let _ = sp_io::crypto::secp256k1_ecdsa_recover(&sig65, &msg32);
	let _ = sp_io::crypto::secp256k1_ecdsa_recover_compressed(&sig65, &msg32);

	#[cfg(feature = "bandersnatch-experimental")]
	{
		let _ = sp_io::crypto::bandersnatch_generate(id, None);
		let _ = sp_io::crypto::bandersnatch_sign(id, &Default::default(), b"msg");
	}

	#[cfg(feature = "bls-experimental")]
	{
		let _ = sp_io::crypto::bls381_generate(id, None);
		let _ =
			sp_io::crypto::bls381_generate_proof_of_possession(id, &Default::default(), b"owner");
		let _ = sp_io::crypto::ecdsa_bls381_generate(id, None);
	}
}

fn touch_misc() {
	let mut buf = [0u8; 4];
	sp_io::misc::last_cursor(&mut buf);
	sp_io::misc::print_hex(b"hex");
	sp_io::misc::print_num(42);
	sp_io::misc::print_utf8(b"utf8");
	let mut rv_out = [0u8; 4];
	sp_io::misc::runtime_version__raw(&[], &mut rv_out);
	let _ = sp_io::misc::runtime_version(&[]);
}

fn touch_offchain() {
	let request = HttpRequestId(0);
	let ids = [HttpRequestId(0)];
	let deadline = Timestamp::default();
	let kind = StorageKind::PERSISTENT;
	let mut buf = [0u8; 4];
	let mut statuses = [0u32; 4];
	let mut seed32 = [0u8; 32];
	let mut peer_id = NetworkPeerId::default();

	let _ = sp_io::offchain::http_request_add_header(request, "h", "v");
	let _ = sp_io::offchain::http_request_start("GET", "http://x", vec![]);
	let _ = sp_io::offchain::http_request_write_body(request, &[], None);
	let _ = sp_io::offchain::http_response_header_name(request, 0, &mut buf);
	let _ = sp_io::offchain::http_response_header_value(request, 0, &mut buf);
	let _ = sp_io::offchain::http_response_read_body(request, &mut buf, None);
	sp_io::offchain::http_response_wait__raw(&ids, Some(deadline), &mut statuses);
	let _ = sp_io::offchain::is_validator();
	sp_io::offchain::local_storage_clear(kind, &[]);
	let _ = sp_io::offchain::local_storage_compare_and_set(kind, &[], None, &[]);
	let _ = sp_io::offchain::local_storage_read(kind, &[], &mut buf, 0);
	sp_io::offchain::local_storage_set(kind, &[], &[]);
	let _ = sp_io::offchain::network_peer_id(&mut peer_id);
	sp_io::offchain::random_seed__raw(&mut seed32);
	sp_io::offchain::set_authorized_nodes(vec![OpaquePeerId(vec![])], false);
	sp_io::offchain::sleep_until(deadline);
	let _ = sp_io::offchain::submit_transaction(vec![]);
	let _ = sp_io::offchain::timestamp();
	let _ = sp_io::offchain::random_seed();
	let _ = sp_io::offchain::local_storage_get(kind, &[]);
	let _ = sp_io::offchain::http_response_wait(&ids, Some(deadline));
	let _ = sp_io::offchain::http_response_headers(request);
}

fn touch_offchain_index() {
	sp_io::offchain_index::clear(&[]);
	sp_io::offchain_index::set(&[], &[]);
}

fn touch_transaction_index() {
	sp_io::transaction_index::index(0, 0, [0u8; 32]);
	sp_io::transaction_index::renew(0, [0u8; 32]);
}

fn touch_allocator() {
	sp_io::allocator::free(sp_io::allocator::malloc(1));
}

fn touch_logging() {
	sp_io::logging::log(RuntimeInterfaceLogLevel::Error, "sp-io-smoke", b"smoke");
	let _ = sp_io::logging::max_level();
}

fn touch_wasm_tracing() {
	let metadata = WasmMetadata::default();
	let attributes = WasmEntryAttributes::default();
	let _ = sp_io::wasm_tracing::enabled(metadata);
	let _ = sp_io::wasm_tracing::enter_span(attributes.clone());
	sp_io::wasm_tracing::event(attributes);
	sp_io::wasm_tracing::exit(0);
}

fn touch_panic_handler() {
	// `abort_on_panic` is `-> !`; keep it isolated so the following `if false` guard never
	// lets it actually run. Compile-checked only.
	if false {
		sp_io::panic_handler::abort_on_panic("abort");
	}
}
