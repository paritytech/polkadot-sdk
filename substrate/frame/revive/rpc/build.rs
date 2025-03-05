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
use std::process::Command;

/// Get the current branch and commit hash.
fn main() {
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
