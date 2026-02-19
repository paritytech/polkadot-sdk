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

mod command;

// Force the linker to keep the polkadot_jemalloc_shim crate (and its #[global_allocator]).
// Without it, the shim is seen as a dependency that produces no referenced symbols, so the linker
// might drop it. We have seen it happening on CI with rust 1.88.0 and gcc/ld from Ubuntu 24.04 but
// not with rust 1.92.0 and the same linker. It also works without the extern crate declaration on
// both rust 1.88.0 and 1.92.0 when using clang/ld or mold, so it seems to be a combination of rust
// version and linker.
#[cfg(target_os = "linux")]
extern crate polkadot_jemalloc_shim;

use clap::Parser;
use sc_cli::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
	setup_logger();

	command::Command::parse().run()
}

/// Setup logging with `info` as default level. Can be set via `RUST_LOG` env.
fn setup_logger() {
	// Disable these log targets because they are spammy.
	let unwanted_targets =
		&["cranelift_codegen", "wasm_cranelift", "wasmtime_jit", "wasmtime_cranelift", "wasm_jit"];

	let mut env_filter =
		EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	for target in unwanted_targets {
		env_filter = env_filter.add_directive(format!("{}=off", target).parse().unwrap());
	}

	tracing_subscriber::fmt()
		.with_env_filter(env_filter)
		.with_writer(std::io::stderr)
		.init();
}
