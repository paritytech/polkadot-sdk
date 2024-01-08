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

use std::{borrow::Cow, process::Command};

/// Generate the `cargo:` key output
pub fn generate_cargo_keys() {
	let commit = if let Ok(hash) = std::env::var("SUBSTRATE_CLI_GIT_COMMIT_HASH") {
		Cow::from(hash.trim().to_owned())
	} else {
		// We deliberately set the length here to `11` to ensure that
		// the emitted hash is always of the same length; otherwise
		// it can (and will!) vary between different build environments.
		match Command::new("git").args(["rev-parse", "--short=11", "HEAD"]).output() {
			Ok(o) if o.status.success() => {
				let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
				Cow::from(sha)
			},
			Ok(o) => {
				let stderr = String::from_utf8_lossy(&o.stderr).trim().to_owned();
				println!(
					"cargo:warning=Git command failed with status '{}' with message: '{}'",
					o.status, stderr,
				);
				Cow::from("unknown")
			},
			Err(err) => {
				println!("cargo:warning=Failed to execute git command: {}", err);
				Cow::from("unknown")
			},
		}
	};

	println!("cargo:rustc-env=SUBSTRATE_CLI_COMMIT_HASH={commit}");
	println!("cargo:rustc-env=SUBSTRATE_CLI_IMPL_VERSION={}", get_version(&commit))
}

fn get_version(impl_commit: &str) -> String {
	let commit_dash = if impl_commit.is_empty() { "" } else { "-" };

	format!(
		"{}{}{}",
		std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
		commit_dash,
		impl_commit
	)
}

/// Generate `SUBSTRATE_WASMTIME_VERSION`
pub fn generate_wasmtime_version() {
	generate_dependency_version("wasmtime", "SUBSTRATE_WASMTIME_VERSION");
}

fn generate_dependency_version(dep: &str, env_var: &str) {
	// we only care about the root
	match std::process::Command::new("cargo")
		.args(["tree", "--depth=0", "--locked", "--package", dep])
		.output()
	{
		Ok(output) if output.status.success() => {
			let version = String::from_utf8_lossy(&output.stdout);

			// <DEP> vX.X.X
			if let Some(ver) = version.strip_prefix(&format!("{} v", dep)) {
				println!("cargo:rustc-env={}={}", env_var, ver);
			} else {
				println!("cargo:warning=Unexpected result {}", version);
			}
		},

		// command errors out when it could not find the given dependency
		// or when having multiple versions of it
		Ok(output) =>
			println!("cargo:warning=`cargo tree` {}", String::from_utf8_lossy(&output.stderr)),

		Err(err) => println!("cargo:warning=Could not run `cargo tree`: {}", err),
	}
}
