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
use crate::generator::{format_code, TypeGenerator};
use anyhow::Context;
use std::path::Path;

mod generator;
mod open_rpc;
mod printer;

fn main() -> anyhow::Result<()> {
	let specs = generator::read_specs()?;

	let mut generator = TypeGenerator::new();
	generator.collect_extra_type("TransactionUnsigned");

	let out_dir = if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
		Path::new(&dir).join("../src")
	} else {
		"../src".into()
	}
	.canonicalize()
	.with_context(|| "Failed to find the api directory")?;

	let out = out_dir.join("rpc_methods_gen.rs");
	println!("Generating rpc_methods at {out:?}");
	format_and_write_file(&out, &generator.generate_rpc_methods(&specs))
		.with_context(|| format!("Failed to generate code to {out:?}"))?;

	let out_dir = if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
		Path::new(&dir).join("../../src/evm/api")
	} else {
		"../../src/evm/api".into()
	}
	.canonicalize()
	.with_context(|| "Failed to find the api directory")?;

	let out = std::fs::canonicalize(out_dir.join("rpc_types_gen.rs"))?;
	println!("Generating rpc_types at {out:?}");
	format_and_write_file(&out, &generator.generate_types(&specs))
		.with_context(|| format!("Failed to generate code to {out:?}"))?;

	Ok(())
}

fn format_and_write_file(path: &Path, content: &str) -> anyhow::Result<()> {
	let code = format_code(content)?;
	std::fs::write(path, code).expect("Unable to write file");
	Ok(())
}
