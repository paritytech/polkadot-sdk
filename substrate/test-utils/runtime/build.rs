// This file is part of Substrate.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
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

use wasm_builder_runner::WasmBuilder;

fn main() {
	WasmBuilder::new()
		.with_current_project()
		.with_wasm_builder_from_crates_or_path("1.0.11", "../../utils/wasm-builder")
		.export_heap_base()
		// Note that we set the stack-size to 1MB explicitly even though it is set
		// to this value by default. This is because some of our tests (`restoration_of_globals`)
		// depend on the stack-size.
		.append_to_rust_flags("-Clink-arg=-zstack-size=1048576")
		.import_memory()
		.build()
}
