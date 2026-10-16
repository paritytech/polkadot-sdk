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

//! Tests pinning the JAM host-call index table and the node-side `HostFunctions` coverage.

use super::host_call_indices;
use sp_jam_runtime_interface::{FunctionContext, HostFunctions, Registers};

/// The full expected index table, in the plan's order.
///
/// Reserved ranges are NOT emitted entries and do not appear here: index 204 (log; logging is
/// a no-op), 312-316 (bandersnatch/bls) and 341-344 (proof size, additional data, jam state
/// read; owned outside sp-io).
const EXPECTED_INDICES: &[(u32, &str)] = &[
	(0, "ext_pvm_gas"),
	(1, "ext_pvm_grow_heap"),
	(2, "ext_pvm_fetch"),
	(7, "ext_pvm_historical_lookup"),
	(8, "ext_pvm_export"),
	(200, "ext_service_set_parent_head_hash"),
	(201, "ext_service_set_head"),
	(202, "ext_service_send_upward_message"),
	(203, "ext_service_report_error"),
	(301, "ext_storage_read"),
	(302, "ext_storage_set"),
	(303, "ext_storage_clear"),
	(304, "ext_storage_exists"),
	(305, "ext_storage_clear_prefix"),
	(306, "ext_storage_append"),
	(307, "ext_storage_root"),
	(308, "ext_storage_next_key"),
	(309, "ext_storage_start_transaction"),
	(310, "ext_storage_rollback_transaction"),
	(311, "ext_storage_commit_transaction"),
	(320, "ext_default_child_storage_read"),
	(321, "ext_default_child_storage_set"),
	(322, "ext_default_child_storage_clear"),
	(323, "ext_default_child_storage_storage_kill"),
	(324, "ext_default_child_storage_exists"),
	(325, "ext_default_child_storage_clear_prefix"),
	(326, "ext_default_child_storage_root"),
	(327, "ext_default_child_storage_next_key"),
	(328, "ext_crypto_ed25519_generate"),
	(329, "ext_crypto_ed25519_public_keys"),
	(330, "ext_crypto_ed25519_sign"),
	(331, "ext_crypto_sr25519_generate"),
	(332, "ext_crypto_sr25519_public_keys"),
	(333, "ext_crypto_sr25519_sign"),
	(334, "ext_crypto_ecdsa_generate"),
	(335, "ext_crypto_ecdsa_public_keys"),
	(336, "ext_crypto_ecdsa_sign"),
	(337, "ext_crypto_ecdsa_sign_prehashed"),
	(340, "ext_input_read"),
	(345, "ext_offchain_index_set"),
	(346, "ext_offchain_index_clear"),
	(347, "ext_transaction_index_index"),
	(348, "ext_transaction_index_renew"),
	(350, "ext_offchain_is_validator"),
	(351, "ext_offchain_submit_transaction"),
	(352, "ext_offchain_network_peer_id"),
	(353, "ext_offchain_timestamp"),
	(354, "ext_offchain_sleep_until"),
	(355, "ext_offchain_random_seed"),
	(356, "ext_offchain_local_storage_set"),
	(357, "ext_offchain_local_storage_clear"),
	(358, "ext_offchain_local_storage_compare_and_set"),
	(359, "ext_offchain_local_storage_read"),
	(360, "ext_offchain_http_request_start"),
	(361, "ext_offchain_http_request_add_header"),
	(362, "ext_offchain_http_request_write_body"),
	(363, "ext_offchain_http_response_wait"),
	(364, "ext_offchain_http_response_header_name"),
	(365, "ext_offchain_http_response_header_value"),
	(366, "ext_offchain_http_response_read_body"),
	(367, "ext_offchain_set_authorized_nodes"),
];

/// The names of every host call with a node side (the "Node side: yes" rows of the table),
/// each named `ext_<module>_<name>`.
const EXPECTED_NODE_CALLS: &[&str] = &[
	"ext_crypto_ecdsa_generate",
	"ext_crypto_ecdsa_public_keys",
	"ext_crypto_ecdsa_sign",
	"ext_crypto_ecdsa_sign_prehashed",
	"ext_crypto_ed25519_generate",
	"ext_crypto_ed25519_public_keys",
	"ext_crypto_ed25519_sign",
	"ext_crypto_sr25519_generate",
	"ext_crypto_sr25519_public_keys",
	"ext_crypto_sr25519_sign",
	"ext_default_child_storage_clear",
	"ext_default_child_storage_clear_prefix",
	"ext_default_child_storage_exists",
	"ext_default_child_storage_next_key",
	"ext_default_child_storage_read",
	"ext_default_child_storage_root",
	"ext_default_child_storage_set",
	"ext_default_child_storage_storage_kill",
	"ext_input_read",
	"ext_offchain_http_request_add_header",
	"ext_offchain_http_request_start",
	"ext_offchain_http_request_write_body",
	"ext_offchain_http_response_header_name",
	"ext_offchain_http_response_header_value",
	"ext_offchain_http_response_read_body",
	"ext_offchain_http_response_wait",
	"ext_offchain_is_validator",
	"ext_offchain_local_storage_clear",
	"ext_offchain_local_storage_compare_and_set",
	"ext_offchain_local_storage_read",
	"ext_offchain_local_storage_set",
	"ext_offchain_network_peer_id",
	"ext_offchain_random_seed",
	"ext_offchain_set_authorized_nodes",
	"ext_offchain_sleep_until",
	"ext_offchain_submit_transaction",
	"ext_offchain_timestamp",
	"ext_offchain_index_clear",
	"ext_offchain_index_set",
	"ext_storage_append",
	"ext_storage_clear",
	"ext_storage_clear_prefix",
	"ext_storage_commit_transaction",
	"ext_storage_exists",
	"ext_storage_next_key",
	"ext_storage_read",
	"ext_storage_rollback_transaction",
	"ext_storage_root",
	"ext_storage_set",
	"ext_storage_start_transaction",
	"ext_transaction_index_index",
	"ext_transaction_index_renew",
];

#[test]
fn host_call_indices_match_spec() {
	let indices = host_call_indices();
	let mut seen = std::collections::BTreeMap::new();
	for (index, name) in &indices {
		if let Some(previous) = seen.insert(index, name) {
			panic!("duplicate host-call index {}: '{}' clashes with '{}'", index, previous, name);
		}
	}

	let max = indices.iter().map(|(index, _)| *index).max().unwrap_or(0);
	assert!(max <= 1023, "host-call index {} exceeds the 1023 limit", max);

	assert_eq!(
		indices, EXPECTED_INDICES,
		"host call indices must equal the full index table of the plan",
	);
}

#[test]
fn host_functions_cover_node_calls() {
	let mut names = <super::HostFunctions as HostFunctions>::host_functions()
		.iter()
		.map(|f| f.name())
		.collect::<Vec<_>>();
	names.sort();

	let mut expected = EXPECTED_NODE_CALLS.to_vec();
	expected.sort();

	assert_eq!(
		names, expected,
		"the registered host functions must equal the \"Node side: yes\" rows of the table",
	);
}

/// The argument registers of a host call.
struct Args<'a>(&'a [i64]);

impl Registers for Args<'_> {
	fn arg(&self, index: usize) -> u64 {
		self.0[index] as u64
	}
}

/// A guest memory for driving the host calls through [`HostFunction::execute`].
struct GuestMemory {
	memory: Vec<u8>,
	next: usize,
}

impl GuestMemory {
	fn new() -> Self {
		// Keep address `0` unused, it is the null pointer.
		Self { memory: vec![0; 4096], next: 8 }
	}

	/// Copies `data` into the guest memory, returning its address and length.
	fn put(&mut self, data: &[u8]) -> (i64, i64) {
		let address = self.next;
		self.memory[address..address + data.len()].copy_from_slice(data);
		self.next += data.len().next_multiple_of(8);
		(address as i64, data.len() as i64)
	}

	/// Allocates `len` zeroed bytes, returning their address and length.
	fn alloc(&mut self, len: usize) -> (i64, i64) {
		self.put(&vec![0; len])
	}

	fn get(&self, (address, len): (i64, i64)) -> &[u8] {
		&self.memory[address as usize..(address + len) as usize]
	}

	fn call(&mut self, name: &str, args: &[i64]) -> Result<Option<i64>, String> {
		let function = <super::HostFunctions as HostFunctions>::host_functions()
			.into_iter()
			.find(|function| function.name() == name)
			.unwrap_or_else(|| panic!("host function {name} exists"));
		function
			.execute(self, &Args(args))
			.map(|result| result.map(|result| result as i64))
	}
}

impl FunctionContext for GuestMemory {
	fn read_memory_into(&mut self, ptr: *const u8, dest: &mut [u8]) -> Result<(), String> {
		let start = ptr as usize;
		let data = self.memory.get(start..start + dest.len()).ok_or("out of bounds")?;
		dest.copy_from_slice(data);
		Ok(())
	}

	fn write_memory(&mut self, ptr: *mut u8, data: &[u8]) -> Result<(), String> {
		let start = ptr as usize;
		self.memory
			.get_mut(start..start + data.len())
			.ok_or("out of bounds")?
			.copy_from_slice(data);
		Ok(())
	}
}

#[test]
fn storage_host_calls_work() {
	let mut ext = super::TestExternalities::default();
	ext.execute_with(|| {
		let mut memory = GuestMemory::new();
		let key = memory.put(b"key");
		let value = memory.put(b"hello world");
		assert_eq!(memory.call("ext_storage_set", &[key.0, key.1, value.0, value.1]), Ok(None));
		assert_eq!(super::storage::get(b"key").as_deref(), Some(&b"hello world"[..]));

		assert_eq!(memory.call("ext_storage_exists", &[key.0, key.1]), Ok(Some(1)));

		// Read with an offset into a buffer that is too small; `allow_partial` copies a prefix.
		let out = memory.alloc(4);
		assert_eq!(
			memory.call("ext_storage_read", &[key.0, key.1, out.0, out.1, 6, 1]),
			Ok(Some(5))
		);
		assert_eq!(memory.get(out), b"worl");

		// Without `allow_partial` the buffer stays untouched.
		let out = memory.put(b"xxxx");
		assert_eq!(
			memory.call("ext_storage_read", &[key.0, key.1, out.0, out.1, 0, 0]),
			Ok(Some(11))
		);
		assert_eq!(memory.get(out), b"xxxx");

		// A missing key returns `None`.
		let missing = memory.put(b"missing");
		assert_eq!(
			memory.call("ext_storage_read", &[missing.0, missing.1, out.0, out.1, 0, 1]),
			Ok(Some(-1))
		);

		let next = memory.alloc(16);
		let empty = memory.put(b"");
		assert_eq!(
			memory.call("ext_storage_next_key", &[empty.0, empty.1, next.0, next.1]),
			Ok(Some(5))
		);
		assert_eq!(&memory.get(next)[..5], b":code");

		// `limit = -1` is `None`.
		let prefix = memory.put(b"ke");
		assert_eq!(memory.call("ext_storage_clear_prefix", &[prefix.0, prefix.1, -1]), Ok(Some(0)));
		assert_eq!(super::storage::get(b"key"), None);

		// An invalid `Option<u32>` is rejected.
		assert_eq!(
			memory.call("ext_storage_clear_prefix", &[prefix.0, prefix.1, -2]),
			Err("invalid `Option<u32>`: -2".into())
		);
	});
}

#[test]
fn host_calls_reject_invalid_guest_memory() {
	let mut ext = super::TestExternalities::default();
	ext.execute_with(|| {
		let mut memory = GuestMemory::new();
		let value = memory.put(b"value");

		// A pointer outside of the 32-bit guest address space.
		assert_eq!(
			memory.call("ext_storage_set", &[1 << 32, 1, value.0, value.1]),
			Err("out of bounds".into())
		);

		// A memory region outside of the guest memory.
		assert_eq!(
			memory.call("ext_storage_set", &[4000, 200, value.0, value.1]),
			Err("out of bounds".into())
		);

		// A length outside of the 32-bit guest address space.
		assert_eq!(
			memory.call("ext_storage_set", &[value.0, 1 << 32, value.0, value.1]),
			Err("length 4294967296 exceeds the guest address space".into())
		);

		assert_eq!(super::storage::get(b"value"), None);
	});
}

#[test]
fn crypto_host_calls_work() {
	use sp_core::{crypto::KeyTypeId, ed25519};
	use sp_keystore::{testing::MemoryKeystore, KeystoreExt};

	const ID: KeyTypeId = KeyTypeId(*b"test");

	let mut ext = super::TestExternalities::default();
	ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	ext.execute_with(|| {
		let mut memory = GuestMemory::new();
		let id = i64::from(u32::from_le_bytes(ID.0));

		// Generate from a seed and without one (null pointer).
		let seed = memory.put(b"//Alice");
		let public = memory.alloc(32);
		assert_eq!(
			memory.call("ext_crypto_ed25519_generate", &[id, seed.0, seed.1, public.0]),
			Ok(None)
		);
		let alice = ed25519::Public::from_raw(memory.get(public).try_into().unwrap());
		assert_eq!(alice, super::crypto::ed25519_generate(ID, Some(b"//Alice".to_vec())));
		let other = memory.alloc(32);
		assert_eq!(memory.call("ext_crypto_ed25519_generate", &[id, 0, 0, other.0]), Ok(None));
		assert_ne!(memory.get(other), &[0; 32]);

		// Probe the size, then fetch the keys.
		let none = memory.alloc(0);
		assert_eq!(
			memory.call("ext_crypto_ed25519_public_keys", &[id, none.0, none.1]),
			Ok(Some(64))
		);
		let keys = memory.alloc(64);
		assert_eq!(
			memory.call("ext_crypto_ed25519_public_keys", &[id, keys.0, keys.1]),
			Ok(Some(64))
		);
		assert!(memory.get(keys).chunks(32).any(|key| key == AsRef::<[u8]>::as_ref(&alice)));

		let message = memory.put(b"message");
		let signature = memory.alloc(64);
		assert_eq!(
			memory.call(
				"ext_crypto_ed25519_sign",
				&[id, public.0, message.0, message.1, signature.0]
			),
			Ok(Some(0))
		);
		let signature = ed25519::Signature::from_raw(memory.get(signature).try_into().unwrap());
		assert!(super::crypto::ed25519_verify(&signature, b"message", &alice));

		// Signing with an unknown key fails.
		let unknown = memory.alloc(32);
		let out = memory.alloc(64);
		assert_eq!(
			memory.call("ext_crypto_ed25519_sign", &[id, unknown.0, message.0, message.1, out.0]),
			Ok(Some(-1))
		);
	});
}

#[test]
fn offchain_local_storage_works() {
	use sp_core::offchain::{testing, OffchainDbExt, StorageKind};

	let (offchain, _state) = testing::TestOffchainExt::new();
	let mut ext = super::TestExternalities::default();
	ext.register_extension(OffchainDbExt::new(offchain));
	ext.execute_with(|| {
		let mut memory = GuestMemory::new();
		let kind = StorageKind::PERSISTENT as i64;
		let key = memory.put(b"key");
		let value = memory.put(b"value");
		assert_eq!(
			memory.call("ext_offchain_local_storage_set", &[kind, key.0, key.1, value.0, value.1]),
			Ok(None)
		);

		// Compare-and-set with `None` (null pointer) as the old value fails, the key exists.
		let new = memory.put(b"new");
		assert_eq!(
			memory.call(
				"ext_offchain_local_storage_compare_and_set",
				&[kind, key.0, key.1, 0, 0, new.0, new.1]
			),
			Ok(Some(0))
		);
		assert_eq!(
			memory.call(
				"ext_offchain_local_storage_compare_and_set",
				&[kind, key.0, key.1, value.0, value.1, new.0, new.1]
			),
			Ok(Some(1))
		);

		let out = memory.alloc(8);
		assert_eq!(
			memory.call("ext_offchain_local_storage_read", &[kind, key.0, key.1, out.0, out.1, 0]),
			Ok(Some(3))
		);
		assert_eq!(&memory.get(out)[..3], b"new");

		assert_eq!(
			memory.call("ext_offchain_local_storage_set", &[7, key.0, key.1, value.0, value.1]),
			Err("invalid storage kind: 7".into())
		);
	});
}
