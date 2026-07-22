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

use sp_virtualization::{
	tests::{make_handler, run_loop, RunResult, GAS_MAX},
	Module, ModuleError,
};

fn setup() -> sp_io::TestExternalities {
	sp_tracing::try_init_simple();
	let mut ext = sp_io::TestExternalities::default();
	ext.register_extension(sc_virtualization::default_extension());
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

/// `from_bytes` compiles, caches under the identifier, and the cached module runs.
#[test]
fn compile_caches_and_runs() {
	let program = binary();
	let key = b"some-cache-key";
	setup().execute_with(|| {
		// Compile and cache under `key`.
		Module::from_bytes(program, Some(key)).unwrap();
		// A later lookup hits the cache and yields a runnable module.
		let module = Module::lookup(key).unwrap();
		let instance = module.instantiate().unwrap();
		let execution = instance.prepare(b"counter").unwrap();
		let mut gas_left = GAS_MAX;
		let mut counter: u64 = 0;
		let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
		assert!(matches!(result, RunResult::Ok(_)));
		assert_eq!(counter, 8);
	});
}

/// `Module::lookup` is a pure cache lookup: it misses until a cached compile and hits after,
/// never reading storage.
#[test]
fn lookup_is_pure() {
	let program = binary();
	setup().execute_with(|| {
		let key: &[u8] = b"not-yet-compiled";
		// Nothing is cached under `key` yet.
		assert!(matches!(Module::lookup(key), Err(ModuleError::NotCached)));
		// After a cached compile, lookup hits.
		Module::from_bytes(program, Some(key)).unwrap();
		assert!(Module::lookup(key).is_ok());
	});
}

/// `from_bytes` with `None` compiles without populating the cache.
#[test]
fn compile_none_skips_cache() {
	let program = binary();
	let key: &[u8] = b"would-be-key";
	setup().execute_with(|| {
		Module::from_bytes(program, None).unwrap();
		assert!(matches!(Module::lookup(key), Err(ModuleError::NotCached)));
	});
}

/// Invalid program bytes are rejected at compile time.
#[test]
fn compile_invalid_image() {
	setup().execute_with(|| {
		assert!(matches!(
			Module::from_bytes(b"this is not a valid polkavm program", None),
			Err(ModuleError::InvalidImage)
		));
	});
}
