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
use std::{env, fs, path::Path, process::Command};

fn main() {
	generate_git_revision();
	build_ah_westend_wasm();
}

fn generate_git_revision() {
	let output = Command::new("rustc")
		.arg("--version")
		.output()
		.expect("cannot get the current rustc version");
	// Exports the default rustc --version output:
	// e.g. rustc 1.83.0 (90b35a623 2024-11-26)
	// into the usual Ethereum web3_clientVersion format
	// e.g. rustc1.83.0
	let rustc_version = String::from_utf8_lossy(&output.stdout)
		.split_whitespace()
		.take(2)
		.collect::<Vec<_>>()
		.join("");
	let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

	let (branch, id) = if let Ok(repo) = git2::Repository::open("../../../..") {
		let head = repo.head().expect("should have head");
		let commit = head.peel_to_commit().expect("should have commit");
		let branch = head.shorthand().unwrap_or("unknown").to_string();
		let id = &commit.id().to_string()[..7];
		(branch, id.to_string())
	} else {
		("unknown".to_string(), "unknown".to_string())
	};

	println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");
	println!("cargo:rustc-env=TARGET={target}");
	println!("cargo:rustc-env=GIT_REVISION={branch}-{id}");
}

fn build_ah_westend_wasm() {
	let manifest_dir = env::var("CARGO_MANIFEST_DIR")
		.expect("`CARGO_MANIFEST_DIR` is always set for `build.rs` files; qed");

	let runtime_cargo_toml = Path::new(&manifest_dir)
		.parent()
		.and_then(|p| p.parent())
		.and_then(|p| p.parent())
		.and_then(|p| p.parent())
		.unwrap()
		.join("cumulus/parachains/runtimes/assets/asset-hub-westend/Cargo.toml");

	substrate_wasm_builder::WasmBuilder::new()
		.with_project(runtime_cargo_toml.to_str().expect("Invalid path"))
		.unwrap()
		.build();

	let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
		Path::new(&manifest_dir)
			.join("../../../../target")
			.to_str()
			.unwrap()
			.to_string()
	});

	let wasm_path = Path::new(&target_dir)
		.join(env::var("PROFILE").unwrap_or_else(|_| "debug".to_string()))
		.join("wbuild/asset-hub-westend-runtime/asset_hub_westend_runtime.wasm");

	let symlink_path = Path::new(&manifest_dir).join("asset_hub_westend_runtime.wasm");

	// Remove existing symlink/file if it exists
	let _ = fs::remove_file(&symlink_path);

	// Create symlink
	#[cfg(unix)]
	std::os::unix::fs::symlink(&wasm_path, &symlink_path)
		.expect("Failed to create symlink to WASM file");

	#[cfg(windows)]
	std::os::windows::fs::symlink_file(&wasm_path, &symlink_path)
		.expect("Failed to create symlink to WASM file");
}
