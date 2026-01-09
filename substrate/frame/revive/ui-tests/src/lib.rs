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

mod pallet_dummy;
pub mod runtime;

#[test]
fn precompile_ui() {
	// NB: Tests like this compare compiler output to a reference output. This is compiler dependent.
	// If your local compiler version is different from the one used to generate the reference output,
	// the test may fail even if the code is correct.
	let t = trybuild::TestCases::new();
	let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/precompiles_ui.rs");
	t.compile_fail(path.to_str().unwrap());
}
