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

//! Compile text fixtures to PolkaVM binaries.

#[path = "src/builder.rs"]
mod builder;

use anyhow::{Context, Result};
use builder::{
	collect_entries, compile_solidity_contracts, create_cargo_toml, generate_fixture_location,
	invoke_build, write_output, ContractType,
};
use std::{env, fs, path::PathBuf};

const OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_RUSTUP_TOOLCHAIN";
const OVERRIDE_STRIP_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_STRIP";
const OVERRIDE_OPTIMIZE_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_OPTIMIZE";
/// Do not build the fixtures, they will resolve to `None`.
///
/// Depending on the usage, they will probably panic at runtime.
const SKIP_PALLET_REVIVE_FIXTURES: &str = "SKIP_PALLET_REVIVE_FIXTURES";

pub fn main() -> Result<()> {
	// input pathes
	let fixtures_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")?.into();
	let contracts_dir = fixtures_dir.join("contracts");

	// output pathes
	let out_dir: PathBuf =
		env::var("OUT_DIR").context("Failed to fetch `OUT_DIR` env variable")?.into();
	let out_fixtures_dir = out_dir.join("fixtures");
	let out_build_dir = out_dir.join("build");
	fs::create_dir_all(&out_fixtures_dir).context("Failed to create output fixture directory")?;
	fs::create_dir_all(&out_build_dir).context("Failed to create output build directory")?;

	println!("cargo::rerun-if-env-changed={OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_STRIP_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_OPTIMIZE_ENV_VAR}");

	// the fixtures have a dependency on the uapi crate
	println!("cargo::rerun-if-changed={}", fixtures_dir.display());
	let uapi_dir = fixtures_dir.parent().expect("parent dir exits; qed").join("uapi");
	if uapi_dir.exists() {
		println!("cargo::rerun-if-changed={}", uapi_dir.display());
	}

	let entries = collect_entries(&contracts_dir);
	if entries.is_empty() {
		return Ok(());
	}

	if env::var(SKIP_PALLET_REVIVE_FIXTURES).is_err() {
		// Compile Rust contracts
		let rust_entries: Vec<_> = entries
			.iter()
			.filter(|e| matches!(e.contract_type, ContractType::Rust))
			.collect();
		if !rust_entries.is_empty() {
			create_cargo_toml(Some(&fixtures_dir), rust_entries.into_iter(), &out_build_dir)?;
			invoke_build(&out_build_dir)?;
			write_output(&out_build_dir, &out_fixtures_dir, entries.clone())?;
		}

		// Compile Solidity contracts
		compile_solidity_contracts(&contracts_dir, &out_fixtures_dir, &entries)?;
	}

	// Generate fixture_location.rs with sol! macros
	generate_fixture_location(&out_dir, &out_fixtures_dir, &entries)?;

	Ok(())
}
