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

use std::io::Write;

/// We start with version 2 instead of 0 when adding the pallet.
///
/// Because otherwise we can't test any migrations since they require the storage version
/// to be lower than the pallet version in order to be triggerd. With the pallet version
/// at the minimum (0) this would not work.
const LOWEST_STORAGE_VERSION: u16 = 2;

/// Get the latest migration version.
///
/// Find the highest version number from the available migration files.
/// Each migration file should follow the naming convention `vXX.rs`, where `XX` is the version
/// number.
fn get_latest_version() -> u16 {
	let Ok(dir) = std::fs::read_dir("src/migration") else { return LOWEST_STORAGE_VERSION };
	dir.filter_map(|entry| {
		let file_name = entry.as_ref().ok()?.file_name();
		let file_name = file_name.to_str()?;
		if file_name.starts_with('v') && file_name.ends_with(".rs") {
			let version = &file_name[1..&file_name.len() - 3];
			let version = version.parse::<u16>().ok()?;

			// Ensure that the version matches the one defined in the file.
			let path = entry.unwrap().path();
			let file_content = std::fs::read_to_string(&path).ok()?;
			assert!(
				file_content.contains(&format!("const VERSION: u16 = {}", version)),
				"Invalid MigrationStep::VERSION in {:?}",
				path
			);

			return Some(version)
		}
		None
	})
	.max()
	.unwrap_or(LOWEST_STORAGE_VERSION)
}

/// Generates a module that exposes the latest migration version, and the benchmark migrations type.
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let out_dir = std::env::var("OUT_DIR")?;
	let path = std::path::Path::new(&out_dir).join("migration_codegen.rs");
	let mut f = std::fs::File::create(path)?;
	let version = get_latest_version();
	write!(
		f,
		"
         pub mod codegen {{
		  use crate::NoopMigration;
		  /// The latest migration version, pulled from the latest migration file.
		  pub const LATEST_MIGRATION_VERSION: u16 = {version};
		  /// The Migration Steps used for benchmarking the migration framework.
		  pub type BenchMigrations = (NoopMigration<{}>, NoopMigration<{version}>);
		}}",
		version - 1,
	)?;

	Ok(())
}
