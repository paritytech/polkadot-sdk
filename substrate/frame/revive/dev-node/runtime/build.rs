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

fn main() {
	#[cfg(feature = "std")]
	{
		let mut builder = polkadot_sdk::substrate_wasm_builder::WasmBuilder::init_with_defaults();
		if std::env::var_os("CARGO_CFG_REVIVE_JIT").is_some() {
			builder = builder.append_to_rust_flags("--cfg revive_jit");
		}
		builder.build();
	}
}
