// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use wasm_builder_runner::{build_current_project_with_rustflags, WasmBuilderSource};

fn main() {
	build_current_project_with_rustflags(
		"wasm_binary.rs",
		WasmBuilderSource::CratesOrPath {
			path: "../../utils/wasm-builder",
			version: "1.0.8",
		},
		// Note that we set the stack-size to 1MB explicitly even though it is set
		// to this value by default. This is because some of our tests (`restoration_of_globals`)
		// depend on the stack-size.
		//
		// The --export=__heap_base instructs LLD to export __heap_base as a global variable, which
		// is used by the external memory allocator.
		"-Clink-arg=-zstack-size=1048576 \
		-Clink-arg=--export=__heap_base",
	);
}
