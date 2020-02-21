// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! This is part of the Substrate runtime.

#![warn(missing_docs)]

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]

#![cfg_attr(feature = "std",
   doc = "Substrate runtime standard library as compiled when linked with Rust's standard library.")]
#![cfg_attr(not(feature = "std"),
   doc = "Substrate's runtime standard library as compiled without Rust's standard library.")]

use sp_std::vec::Vec;

#[cfg(feature = "std")]
use sp_std::ops::Deref;

#[cfg(feature = "std")]
use sp_core::{
	crypto::Pair,
	traits::{KeystoreExt, CallInWasmExt},
	offchain::{OffchainExt, TransactionPoolExt},
	hexdisplay::HexDisplay,
	storage::{ChildStorageKey, ChildInfo},
};

use sp_core::{
	crypto::KeyTypeId, ed25519, sr25519, H256, LogLevel,
	offchain::{
		Timestamp, HttpRequestId, HttpRequestStatus, HttpError, StorageKind, OpaqueNetworkState,
	},
};

#[cfg(feature = "std")]
use sp_trie::{TrieConfiguration, trie_types::Layout};

use sp_runtime_interface::{runtime_interface, Pointer};

use codec::{Encode, Decode};

#[cfg(feature = "std")]
use sp_externalities::{ExternalitiesExt, Externalities};

/// Error verifying ECDSA signature
#[derive(Encode, Decode)]
pub enum EcdsaVerifyError {
	/// Incorrect value of R or S
	BadRS,
	/// Incorrect value of V
	BadV,
	/// Invalid signature
	BadSignature,
}

/// Returns a `ChildStorageKey` if the given `storage_key` slice is a valid storage
/// key or panics otherwise.
///
/// Panicking here is aligned with what the `without_std` environment would do
/// in the case of an invalid child storage key.
#[cfg(feature = "std")]
fn child_storage_key_or_panic(storage_key: &[u8]) -> ChildStorageKey {
	match ChildStorageKey::from_slice(storage_key) {
		Some(storage_key) => storage_key,
		None => panic!("child storage key is invalid"),
	}
}

/// Interface for accessing the storage from within the runtime.
#[runtime_interface]
pub trait Storage {
	/// Returns the data for `key` in the storage or `None` if the key can not be found.
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.storage(key).map(|s| s.to_vec())
	}

	/// All Child api uses :
	/// - A `child_storage_key` to define the anchor point for the child proof
	/// (commonly the location where the child root is stored in its parent trie).
	/// - A `child_storage_types` to identify the kind of the child type and how its
	/// `child definition` parameter is encoded.
	/// - A `child_definition_parameter` which is the additional information required
	/// to use the child trie. For instance defaults child tries requires this to
	/// contain a collision free unique id.
	///
	/// This function specifically returns the data for `key` in the child storage or `None`
	/// if the key can not be found.
	fn child_get(
		&self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.child_storage(storage_key, child_info, key).map(|s| s.to_vec())
	}

	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	fn read(&self, key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
		self.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = std::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			value.len() as u32
		})
	}

	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	///
	/// See `child_get` for common child api parameters.
	fn child_read(
		&self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
		value_out: &mut [u8],
		value_offset: u32,
	) -> Option<u32> {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.child_storage(storage_key, child_info, key)
			.map(|value| {
				let value_offset = value_offset as usize;
				let data = &value[value_offset.min(value.len())..];
				let written = std::cmp::min(data.len(), value_out.len());
				value_out[..written].copy_from_slice(&data[..written]);
				value.len() as u32
			})
	}

	/// Set `key` to `value` in the storage.
	fn set(&mut self, key: &[u8], value: &[u8]) {
		self.set_storage(key.to_vec(), value.to_vec());
	}

	/// Set `key` to `value` in the child storage denoted by `child_storage_key`.
	///
	/// See `child_get` for common child api parameters.
	fn child_set(
		&mut self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
		value: &[u8],
	) {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.set_child_storage(storage_key, child_info, key.to_vec(), value.to_vec());
	}

	/// Clear the storage of the given `key` and its value.
	fn clear(&mut self, key: &[u8]) {
		self.clear_storage(key)
	}

	/// Clear the given child storage of the given `key` and its value.
	///
	/// See `child_get` for common child api parameters.
	fn child_clear(
		&mut self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
	) {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.clear_child_storage(storage_key, child_info, key);
	}

	/// Clear an entire child storage.
	///
	/// See `child_get` for common child api parameters.
	fn child_storage_kill(
		&mut self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
	) {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.kill_child_storage(storage_key, child_info);
	}

	/// Check whether the given `key` exists in storage.
	fn exists(&self, key: &[u8]) -> bool {
		self.exists_storage(key)
	}

	/// Check whether the given `key` exists in storage.
	///
	/// See `child_get` for common child api parameters.
	fn child_exists(
		&self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
	) -> bool {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.exists_child_storage(storage_key, child_info, key)
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	fn clear_prefix(&mut self, prefix: &[u8]) {
		Externalities::clear_prefix(*self, prefix)
	}

	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// See `child_get` for common child api parameters.
	fn child_clear_prefix(
		&mut self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		prefix: &[u8],
	) {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.clear_child_prefix(storage_key, child_info, prefix);
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns the SCALE encoded hash.
	fn root(&mut self) -> Vec<u8> {
		self.storage_root()
	}

	/// "Commit" all existing operations and compute the resulting child storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns the SCALE encoded hash.
	///
	/// See `child_get` for common child api parameters.
	fn child_root(
		&mut self,
		child_storage_key: &[u8],
	) -> Vec<u8> {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		self.child_storage_root(storage_key)
	}

	/// "Commit" all existing operations and get the resulting storage change root.
	/// `parent_hash` is a SCALE encoded hash.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns an `Option` that holds the SCALE encoded hash.
	fn changes_root(&mut self, parent_hash: &[u8]) -> Option<Vec<u8>> {
		self.storage_changes_root(parent_hash)
			.expect("Invalid `parent_hash` given to `changes_root`.")
	}

	/// Get the next key in storage after the given one in lexicographic order.
	fn next_key(&mut self, key: &[u8]) -> Option<Vec<u8>> {
		self.next_storage_key(&key)
	}

	/// Get the next key in storage after the given one in lexicographic order in child storage.
	fn child_next_key(
		&mut self,
		child_storage_key: &[u8],
		child_definition: &[u8],
		child_type: u32,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let storage_key = child_storage_key_or_panic(child_storage_key);
		let child_info = ChildInfo::resolve_child_info(child_type, child_definition)
			.expect("Invalid child definition");
		self.next_child_storage_key(storage_key, child_info, key)
	}
}

/// Interface that provides trie related functionality.
#[runtime_interface]
pub trait Trie {
	/// A trie root formed from the iterated items.
	fn blake2_256_root(input: Vec<(Vec<u8>, Vec<u8>)>) -> H256 {
		Layout::<sp_core::Blake2Hasher>::trie_root(input)
	}

	/// A trie root formed from the enumerated items.
	fn blake2_256_ordered_root(input: Vec<Vec<u8>>) -> H256 {
		Layout::<sp_core::Blake2Hasher>::ordered_trie_root(input)
	}
}

/// Interface that provides miscellaneous functions for communicating between the runtime and the node.
#[runtime_interface]
pub trait Misc {
	/// The current relay chain identifier.
	fn chain_id(&self) -> u64 {
		sp_externalities::Externalities::chain_id(*self)
	}

	/// Print a number.
	fn print_num(val: u64) {
		log::debug!(target: "runtime", "{}", val);
	}

	/// Print any valid `utf8` buffer.
	fn print_utf8(utf8: &[u8]) {
		if let Ok(data) = std::str::from_utf8(utf8) {
			log::debug!(target: "runtime", "{}", data)
		}
	}

	/// Print any `u8` slice as hex.
	fn print_hex(data: &[u8]) {
		log::debug!(target: "runtime", "{}", HexDisplay::from(&data));
	}

	/// Extract the runtime version of the given wasm blob by calling `Core_version`.
	///
	/// Returns the SCALE encoded runtime version and `None` if the call failed.
	///
	/// # Performance
	///
	/// Calling this function is very expensive and should only be done very occasionally.
	/// For getting the runtime version, it requires instantiating the wasm blob and calling a
	/// function in this blob.
	fn runtime_version(&mut self, wasm: &[u8]) -> Option<Vec<u8>> {
		// Create some dummy externalities, `Core_version` should not write data anyway.
		let mut ext = sp_state_machine::BasicExternalities::default();

		self.extension::<CallInWasmExt>()
			.expect("No `CallInWasmExt` associated for the current context!")
			.call_in_wasm(wasm, "Core_version", &[], &mut ext)
			.ok()
	}
}

/// Interfaces for working with crypto related types from within the runtime.
#[runtime_interface]
pub trait Crypto {
	/// Returns all `ed25519` public keys for the given key id from the keystore.
	fn ed25519_public_keys(&mut self, id: KeyTypeId) -> Vec<ed25519::Public> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.read()
			.ed25519_public_keys(id)
	}

	/// Generate an `ed22519` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn ed25519_generate(&mut self, id: KeyTypeId, seed: Option<Vec<u8>>) -> ed25519::Public {
		let seed = seed.as_ref().map(|s| std::str::from_utf8(&s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.write()
			.ed25519_generate_new(id, seed)
			.expect("`ed25519_generate` failed")
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn ed25519_sign(
		&mut self,
		id: KeyTypeId,
		pub_key: &ed25519::Public,
		msg: &[u8],
	) -> Option<ed25519::Signature> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.read()
			.ed25519_key_pair(id, &pub_key)
			.map(|k| k.sign(msg))
	}

	/// Verify an `ed25519` signature.
	///
	/// Returns `true` when the verification in successful.
	fn ed25519_verify(
		&self,
		sig: &ed25519::Signature,
		msg: &[u8],
		pub_key: &ed25519::Public,
	) -> bool {
		ed25519::Pair::verify(sig, msg, pub_key)
	}

	/// Returns all `sr25519` public keys for the given key id from the keystore.
	fn sr25519_public_keys(&mut self, id: KeyTypeId) -> Vec<sr25519::Public> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.read()
			.sr25519_public_keys(id)
	}

	/// Generate an `sr22519` key for the given key type using an optional seed and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn sr25519_generate(&mut self, id: KeyTypeId, seed: Option<Vec<u8>>) -> sr25519::Public {
		let seed = seed.as_ref().map(|s| std::str::from_utf8(&s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.write()
			.sr25519_generate_new(id, seed)
			.expect("`sr25519_generate` failed")
	}

	/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn sr25519_sign(
		&mut self,
		id: KeyTypeId,
		pub_key: &sr25519::Public,
		msg: &[u8],
	) -> Option<sr25519::Signature> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.read()
			.sr25519_key_pair(id, &pub_key)
			.map(|k| k.sign(msg))
	}

	/// Verify an `sr25519` signature.
	///
	/// Returns `true` when the verification in successful.
	fn sr25519_verify(sig: &sr25519::Signature, msg: &[u8], pubkey: &sr25519::Public) -> bool {
		sr25519::Pair::verify(sig, msg, pubkey)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	fn secp256k1_ecdsa_recover(
		sig: &[u8; 65],
		msg: &[u8; 32],
	) -> Result<[u8; 64], EcdsaVerifyError> {
		let rs = secp256k1::Signature::parse_slice(&sig[0..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let v = secp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let pubkey = secp256k1::recover(&secp256k1::Message::parse(msg), &rs, &v)
			.map_err(|_| EcdsaVerifyError::BadSignature)?;
		let mut res = [0u8; 64];
		res.copy_from_slice(&pubkey.serialize()[1..65]);
		Ok(res)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	fn secp256k1_ecdsa_recover_compressed(
		sig: &[u8; 65],
		msg: &[u8; 32],
	) -> Result<[u8; 33], EcdsaVerifyError> {
		let rs = secp256k1::Signature::parse_slice(&sig[0..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let v = secp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let pubkey = secp256k1::recover(&secp256k1::Message::parse(msg), &rs, &v)
			.map_err(|_| EcdsaVerifyError::BadSignature)?;
		Ok(pubkey.serialize_compressed())
	}
}

/// Interface that provides functions for hashing with different algorithms.
#[runtime_interface]
pub trait Hashing {
	/// Conduct a 256-bit Keccak hash.
	fn keccak_256(data: &[u8]) -> [u8; 32] {
		sp_core::hashing::keccak_256(data)
	}

	/// Conduct a 256-bit Sha2 hash.
	fn sha2_256(data: &[u8]) -> [u8; 32] {
		sp_core::hashing::sha2_256(data)
	}

	/// Conduct a 128-bit Blake2 hash.
	fn blake2_128(data: &[u8]) -> [u8; 16] {
		sp_core::hashing::blake2_128(data)
	}

	/// Conduct a 256-bit Blake2 hash.
	fn blake2_256(data: &[u8]) -> [u8; 32] {
		sp_core::hashing::blake2_256(data)
	}

	/// Conduct four XX hashes to give a 256-bit result.
	fn twox_256(data: &[u8]) -> [u8; 32] {
		sp_core::hashing::twox_256(data)
	}

	/// Conduct two XX hashes to give a 128-bit result.
	fn twox_128(data: &[u8]) -> [u8; 16] {
		sp_core::hashing::twox_128(data)
	}

	/// Conduct two XX hashes to give a 64-bit result.
	fn twox_64(data: &[u8]) -> [u8; 8] {
		sp_core::hashing::twox_64(data)
	}
}

/// Interface that provides functions to access the offchain functionality.
#[runtime_interface]
pub trait Offchain {
	/// Returns if the local node is a potential validator.
	///
	/// Even if this function returns `true`, it does not mean that any keys are configured
	/// and that the validator is registered in the chain.
	fn is_validator(&mut self) -> bool {
		self.extension::<OffchainExt>()
			.expect("is_validator can be called only in the offchain worker context")
			.is_validator()
	}

	/// Submit an encoded transaction to the pool.
	///
	/// The transaction will end up in the pool.
	fn submit_transaction(&mut self, data: Vec<u8>) -> Result<(), ()> {
		self.extension::<TransactionPoolExt>()
			.expect("submit_transaction can be called only in the offchain call context with
				TransactionPool capabilities enabled")
			.submit_transaction(data)
	}

	/// Returns information about the local node's network state.
	fn network_state(&mut self) -> Result<OpaqueNetworkState, ()> {
		self.extension::<OffchainExt>()
			.expect("network_state can be called only in the offchain worker context")
			.network_state()
	}

	/// Returns current UNIX timestamp (in millis)
	fn timestamp(&mut self) -> Timestamp {
		self.extension::<OffchainExt>()
			.expect("timestamp can be called only in the offchain worker context")
			.timestamp()
	}

	/// Pause the execution until `deadline` is reached.
	fn sleep_until(&mut self, deadline: Timestamp) {
		self.extension::<OffchainExt>()
			.expect("sleep_until can be called only in the offchain worker context")
			.sleep_until(deadline)
	}

	/// Returns a random seed.
	///
	/// This is a truly random, non-deterministic seed generated by host environment.
	/// Obviously fine in the off-chain worker context.
	fn random_seed(&mut self) -> [u8; 32] {
		self.extension::<OffchainExt>()
			.expect("random_seed can be called only in the offchain worker context")
			.random_seed()
	}

	/// Sets a value in the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_set(&mut self, kind: StorageKind, key: &[u8], value: &[u8]) {
		self.extension::<OffchainExt>()
			.expect("local_storage_set can be called only in the offchain worker context")
			.local_storage_set(kind, key, value)
	}

	/// Sets a value in the local storage if it matches current value.
	///
	/// Since multiple offchain workers may be running concurrently, to prevent
	/// data races use CAS to coordinate between them.
	///
	/// Returns `true` if the value has been set, `false` otherwise.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_compare_and_set(
		&mut self,
		kind: StorageKind,
		key: &[u8],
		old_value: Option<Vec<u8>>,
		new_value: &[u8],
	) -> bool {
		self.extension::<OffchainExt>()
			.expect("local_storage_compare_and_set can be called only in the offchain worker context")
			.local_storage_compare_and_set(kind, key, old_value.as_ref().map(|v| v.deref()), new_value)
	}

	/// Gets a value from the local storage.
	///
	/// If the value does not exist in the storage `None` will be returned.
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_get(&mut self, kind: StorageKind, key: &[u8]) -> Option<Vec<u8>> {
		self.extension::<OffchainExt>()
			.expect("local_storage_get can be called only in the offchain worker context")
			.local_storage_get(kind, key)
	}

	/// Initiates a http request given HTTP verb and the URL.
	///
	/// Meta is a future-reserved field containing additional, parity-scale-codec encoded parameters.
	/// Returns the id of newly started request.
	fn http_request_start(
		&mut self,
		method: &str,
		uri: &str,
		meta: &[u8],
	) -> Result<HttpRequestId, ()> {
		self.extension::<OffchainExt>()
			.expect("http_request_start can be called only in the offchain worker context")
			.http_request_start(method, uri, meta)
	}

	/// Append header to the request.
	fn http_request_add_header(
		&mut self,
		request_id: HttpRequestId,
		name: &str,
		value: &str,
	) -> Result<(), ()> {
		self.extension::<OffchainExt>()
			.expect("http_request_add_header can be called only in the offchain worker context")
			.http_request_add_header(request_id, name, value)
	}

	/// Write a chunk of request body.
	///
	/// Writing an empty chunks finalizes the request.
	/// Passing `None` as deadline blocks forever.
	///
	/// Returns an error in case deadline is reached or the chunk couldn't be written.
	fn http_request_write_body(
		&mut self,
		request_id: HttpRequestId,
		chunk: &[u8],
		deadline: Option<Timestamp>,
	) -> Result<(), HttpError> {
		self.extension::<OffchainExt>()
			.expect("http_request_write_body can be called only in the offchain worker context")
			.http_request_write_body(request_id, chunk, deadline)
	}

	/// Block and wait for the responses for given requests.
	///
	/// Returns a vector of request statuses (the len is the same as ids).
	/// Note that if deadline is not provided the method will block indefinitely,
	/// otherwise unready responses will produce `DeadlineReached` status.
	///
	/// Passing `None` as deadline blocks forever.
	fn http_response_wait(
		&mut self,
		ids: &[HttpRequestId],
		deadline: Option<Timestamp>,
	) -> Vec<HttpRequestStatus> {
		self.extension::<OffchainExt>()
			.expect("http_response_wait can be called only in the offchain worker context")
			.http_response_wait(ids, deadline)
	}

	/// Read all response headers.
	///
	/// Returns a vector of pairs `(HeaderKey, HeaderValue)`.
	/// NOTE response headers have to be read before response body.
	fn http_response_headers(&mut self, request_id: HttpRequestId) -> Vec<(Vec<u8>, Vec<u8>)> {
		self.extension::<OffchainExt>()
			.expect("http_response_headers can be called only in the offchain worker context")
			.http_response_headers(request_id)
	}

	/// Read a chunk of body response to given buffer.
	///
	/// Returns the number of bytes written or an error in case a deadline
	/// is reached or server closed the connection.
	/// If `0` is returned it means that the response has been fully consumed
	/// and the `request_id` is now invalid.
	/// NOTE this implies that response headers must be read before draining the body.
	/// Passing `None` as a deadline blocks forever.
	fn http_response_read_body(
		&mut self,
		request_id: HttpRequestId,
		buffer: &mut [u8],
		deadline: Option<Timestamp>,
	) -> Result<u32, HttpError> {
		self.extension::<OffchainExt>()
			.expect("http_response_read_body can be called only in the offchain worker context")
			.http_response_read_body(request_id, buffer, deadline)
			.map(|r| r as u32)
	}
}

/// Wasm only interface that provides functions for calling into the allocator.
#[runtime_interface(wasm_only)]
trait Allocator {
	/// Malloc the given number of bytes and return the pointer to the allocated memory location.
	fn malloc(&mut self, size: u32) -> Pointer<u8> {
		self.allocate_memory(size).expect("Failed to allocate memory")
	}

	/// Free the given pointer.
	fn free(&mut self, ptr: Pointer<u8>) {
		self.deallocate_memory(ptr).expect("Failed to deallocate memory")
	}
}

/// Interface that provides functions for logging from within the runtime.
#[runtime_interface]
pub trait Logging {
	/// Request to print a log message on the host.
	///
	/// Note that this will be only displayed if the host is enabled to display log messages with
	/// given level and target.
	///
	/// Instead of using directly, prefer setting up `RuntimeLogger` and using `log` macros.
	fn log(level: LogLevel, target: &str, message: &[u8]) {
		if let Ok(message) = std::str::from_utf8(message) {
			log::log!(
				target: target,
				log::Level::from(level),
				"{}",
				message,
			)
		}
	}
}

/// Wasm-only interface that provides functions for interacting with the sandbox.
#[runtime_interface(wasm_only)]
pub trait Sandbox {
	/// Instantiate a new sandbox instance with the given `wasm_code`.
	fn instantiate(
		&mut self,
		dispatch_thunk: u32,
		wasm_code: &[u8],
		env_def: &[u8],
		state_ptr: Pointer<u8>,
	) -> u32 {
		self.sandbox()
			.instance_new(dispatch_thunk, wasm_code, env_def, state_ptr.into())
			.expect("Failed to instantiate a new sandbox")
	}

	/// Invoke `function` in the sandbox with `sandbox_idx`.
	fn invoke(
		&mut self,
		instance_idx: u32,
		function: &str,
		args: &[u8],
		return_val_ptr: Pointer<u8>,
		return_val_len: u32,
		state_ptr: Pointer<u8>,
	) -> u32 {
		self.sandbox().invoke(
			instance_idx,
			&function,
			&args,
			return_val_ptr,
			return_val_len,
			state_ptr.into(),
		).expect("Failed to invoke function with sandbox")
	}

	/// Create a new memory instance with the given `initial` and `maximum` size.
	fn memory_new(&mut self, initial: u32, maximum: u32) -> u32 {
		self.sandbox()
			.memory_new(initial, maximum)
			.expect("Failed to create new memory with sandbox")
	}

	/// Get the memory starting at `offset` from the instance with `memory_idx` into the buffer.
	fn memory_get(
		&mut self,
		memory_idx: u32,
		offset: u32,
		buf_ptr: Pointer<u8>,
		buf_len: u32,
	) -> u32 {
		self.sandbox()
			.memory_get(memory_idx, offset, buf_ptr, buf_len)
			.expect("Failed to get memory with sandbox")
	}

	/// Set the memory in the given `memory_idx` to the given value at `offset`.
	fn memory_set(
		&mut self,
		memory_idx: u32,
		offset: u32,
		val_ptr: Pointer<u8>,
		val_len: u32,
	) -> u32 {
		self.sandbox()
			.memory_set(memory_idx, offset, val_ptr, val_len)
			.expect("Failed to set memory with sandbox")
	}

	/// Teardown the memory instance with the given `memory_idx`.
	fn memory_teardown(&mut self, memory_idx: u32) {
		self.sandbox().memory_teardown(memory_idx).expect("Failed to teardown memory with sandbox")
	}

	/// Teardown the sandbox instance with the given `instance_idx`.
	fn instance_teardown(&mut self, instance_idx: u32) {
		self.sandbox().instance_teardown(instance_idx).expect("Failed to teardown sandbox instance")
	}

	/// Get the value from a global with the given `name`. The sandbox is determined by the given
	/// `instance_idx`.
	///
	/// Returns `Some(_)` when the requested global variable could be found.
	fn get_global_val(&mut self, instance_idx: u32, name: &str) -> Option<sp_wasm_interface::Value> {
		self.sandbox().get_global_val(instance_idx, name).expect("Failed to get global from sandbox")
	}
}

/// Allocator used by Substrate when executing the Wasm runtime.
#[cfg(not(feature = "std"))]
struct WasmAllocator;

#[cfg(all(not(feature = "disable_allocator"), not(feature = "std")))]
#[global_allocator]
static ALLOCATOR: WasmAllocator = WasmAllocator;

#[cfg(not(feature = "std"))]
mod allocator_impl {
	use super::*;
	use core::alloc::{GlobalAlloc, Layout};

	unsafe impl GlobalAlloc for WasmAllocator {
		unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
			allocator::malloc(layout.size() as u32)
		}

		unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
			allocator::free(ptr)
		}
	}
}

/// A default panic handler for WASM environment.
#[cfg(all(not(feature = "disable_panic_handler"), not(feature = "std")))]
#[panic_handler]
#[no_mangle]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
	unsafe {
		let message = sp_std::alloc::format!("{}", info);
		logging::log(LogLevel::Error, "runtime", message.as_bytes());
		core::arch::wasm32::unreachable();
	}
}

/// A default OOM handler for WASM environment.
#[cfg(all(not(feature = "disable_oom"), not(feature = "std")))]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
	unsafe {
		logging::log(LogLevel::Error, "runtime", b"Runtime memory exhausted. Aborting");
		core::arch::wasm32::unreachable();
	}
}

/// Type alias for Externalities implementation used in tests.
#[cfg(feature = "std")]
pub type TestExternalities = sp_state_machine::TestExternalities<sp_core::Blake2Hasher, u64>;

/// The host functions Substrate provides for the Wasm runtime environment.
///
/// All these host functions will be callable from inside the Wasm environment.
#[cfg(feature = "std")]
pub type SubstrateHostFunctions = (
	storage::HostFunctions,
	misc::HostFunctions,
	offchain::HostFunctions,
	crypto::HostFunctions,
	hashing::HostFunctions,
	allocator::HostFunctions,
	logging::HostFunctions,
	sandbox::HostFunctions,
	crate::trie::HostFunctions,
);

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::map;
	use sp_state_machine::BasicExternalities;
	use sp_core::storage::Storage;

	#[test]
	fn storage_works() {
		let mut t = BasicExternalities::default();
		t.execute_with(|| {
			assert_eq!(storage::get(b"hello"), None);
			storage::set(b"hello", b"world");
			assert_eq!(storage::get(b"hello"), Some(b"world".to_vec()));
			assert_eq!(storage::get(b"foo"), None);
			storage::set(b"foo", &[1, 2, 3][..]);
		});

		t = BasicExternalities::new(Storage {
			top: map![b"foo".to_vec() => b"bar".to_vec()],
			children: map![],
		});

		t.execute_with(|| {
			assert_eq!(storage::get(b"hello"), None);
			assert_eq!(storage::get(b"foo"), Some(b"bar".to_vec()));
		});
	}

	#[test]
	fn read_storage_works() {
		let mut t = BasicExternalities::new(Storage {
			top: map![b":test".to_vec() => b"\x0b\0\0\0Hello world".to_vec()],
			children: map![],
		});

		t.execute_with(|| {
			let mut v = [0u8; 4];
			assert!(storage::read(b":test", &mut v[..], 0).unwrap() >= 4);
			assert_eq!(v, [11u8, 0, 0, 0]);
			let mut w = [0u8; 11];
			assert!(storage::read(b":test", &mut w[..], 4).unwrap() >= 11);
			assert_eq!(&w, b"Hello world");
		});
	}

	#[test]
	fn clear_prefix_works() {
		let mut t = BasicExternalities::new(Storage {
			top: map![
				b":a".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abcd".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abc".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abdd".to_vec() => b"\x0b\0\0\0Hello world".to_vec()
			],
			children: map![],
		});

		t.execute_with(|| {
			storage::clear_prefix(b":abc");

			assert!(storage::get(b":a").is_some());
			assert!(storage::get(b":abdd").is_some());
			assert!(storage::get(b":abcd").is_none());
			assert!(storage::get(b":abc").is_none());
		});
	}
}
