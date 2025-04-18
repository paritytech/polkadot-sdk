// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#[cfg(feature = "std")]
fn main() {
	use substrate_wasm_builder::WasmBuilder;

	WasmBuilder::build_using_defaults();

	WasmBuilder::init_with_defaults()
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_spec_version_incremented.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("elastic-scaling")
		.import_memory()
		.set_file_name("wasm_binary_elastic_scaling_mvp.rs")
		.build();

	WasmBuilder::new()
		.with_current_project()
		.enable_feature("elastic-scaling")
		.enable_feature("experimental-ump-signals")
		.import_memory()
		.set_file_name("wasm_binary_elastic_scaling.rs")
		.build();

	WasmBuilder::new()
		.with_current_project()
		.enable_feature("elastic-scaling-500ms")
		.enable_feature("experimental-ump-signals")
		.import_memory()
		.set_file_name("wasm_binary_elastic_scaling_500ms.rs")
		.build();

	WasmBuilder::new()
		.with_current_project()
		.enable_feature("elastic-scaling-multi-block-slot")
		.enable_feature("experimental-ump-signals")
		.import_memory()
		.set_file_name("wasm_binary_elastic_scaling_multi_block_slot.rs")
		.build();
}

#[cfg(not(feature = "std"))]
fn main() {}
