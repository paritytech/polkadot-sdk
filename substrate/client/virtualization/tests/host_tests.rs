// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Native host-side tests exercising the [`VirtManager`] backend through the
//! virtualization host functions.

use sc_virtualization::VirtManager;
use sp_virtualization::{
	tests::{make_handler, run_loop, RunResult, GAS_MAX},
	Module, ModuleError,
};

fn setup() -> sp_io::TestExternalities {
	sp_tracing::try_init_simple();
	let mut ext = sp_io::TestExternalities::default();
	ext.register_extension(sp_virtualization::VirtManagerExt::new(VirtManager::default()));
	ext
}

fn binary() -> &'static [u8] {
	sp_virtualization_test_fixture::binary()
}

/// Drives every wasm-callable test through the host implementation in one shot.
#[test]
fn run_all() {
	setup().execute_with(|| sp_virtualization::run_tests(binary()));
}

/// Compile from bytes, then `from_hash` should hit the in-memory cache.
#[test]
fn from_hash_cache_hit() {
	let program = binary();
	setup().execute_with(|| {
		let _module = Module::from_bytes(program).unwrap();
		let hash = sp_crypto_hashing::keccak_256(program);
		let module = Module::from_hash(&hash, b"", b"").unwrap();
		let instance = module.instantiate().unwrap();
		let execution = instance.prepare(b"counter").unwrap();
		let mut gas_left = GAS_MAX;
		let mut counter: u64 = 0;
		let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
		assert!(matches!(result, RunResult::Ok(_)));
		assert_eq!(counter, 8);
	});
}

/// Load code from main trie storage on cache miss.
#[test]
fn from_hash_storage_main_trie() {
	let program = binary();
	let hash = sp_crypto_hashing::keccak_256(program);
	let prefix = b"code:";
	let mut key = prefix.to_vec();
	key.extend_from_slice(&hash);

	let mut ext = setup();
	ext.insert(key, program.to_vec());
	ext.execute_with(|| {
		let module = Module::from_hash(&hash, prefix, b"").unwrap();
		let instance = module.instantiate().unwrap();
		let execution = instance.prepare(b"counter").unwrap();
		let mut gas_left = GAS_MAX;
		let mut counter: u64 = 0;
		let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
		assert!(matches!(result, RunResult::Ok(_)));
		assert_eq!(counter, 8);

		// Second call should hit the cache now.
		let module = Module::from_hash(&hash, prefix, b"").unwrap();
		let instance = module.instantiate().unwrap();
		let execution = instance.prepare(b"counter").unwrap();
		let mut counter: u64 = 0;
		let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
		assert!(matches!(result, RunResult::Ok(_)));
		assert_eq!(counter, 8);
	});
}

/// Load code from child trie storage on cache miss.
#[test]
fn from_hash_storage_child_trie() {
	let program = binary();
	let hash = sp_crypto_hashing::keccak_256(program);
	let prefix = b"code:";
	let child_trie = b"contracts";
	let mut key = prefix.to_vec();
	key.extend_from_slice(&hash);
	let child_info = sp_storage::ChildInfo::new_default(child_trie);

	let mut ext = setup();
	ext.insert_child(child_info, key, program.to_vec());
	ext.execute_with(|| {
		let module = Module::from_hash(&hash, prefix, child_trie).unwrap();
		let instance = module.instantiate().unwrap();
		let execution = instance.prepare(b"counter").unwrap();
		let mut gas_left = GAS_MAX;
		let mut counter: u64 = 0;
		let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
		assert!(matches!(result, RunResult::Ok(_)));
		assert_eq!(counter, 8);
	});
}

/// Code at the storage key does not match the requested hash.
#[test]
fn from_hash_hash_mismatch() {
	let program = binary();
	let hash = sp_crypto_hashing::keccak_256(program);
	let prefix = b"code:";
	let mut key = prefix.to_vec();
	key.extend_from_slice(&hash);

	let mut ext = setup();
	ext.insert(key, b"not the real program".to_vec());
	ext.execute_with(|| {
		assert!(matches!(Module::from_hash(&hash, prefix, b""), Err(ModuleError::HashMismatch)));
	});
}

/// Code at the storage key has the correct hash but is not valid PolkaVM.
#[test]
fn from_hash_invalid_image() {
	let garbage = b"this is not a valid polkavm program";
	let hash = sp_crypto_hashing::keccak_256(garbage);
	let prefix = b"code:";
	let mut key = prefix.to_vec();
	key.extend_from_slice(&hash);

	let mut ext = setup();
	ext.insert(key, garbage.to_vec());
	ext.execute_with(|| {
		assert!(matches!(Module::from_hash(&hash, prefix, b""), Err(ModuleError::InvalidImage)));
	});
}
