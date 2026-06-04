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

	// A runtime with 6s slot duration which only authors one block per slot.
	WasmBuilder::init_with_defaults().build();

	WasmBuilder::init_with_defaults()
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_spec_version_incremented.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("velocity-3")
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_elastic_scaling.rs")
		.build();

	// A runtime with 6s slots and block velocity 12.
	// Coupled with 12 cores it can produce a block every 500ms.
	WasmBuilder::init_with_defaults()
		.enable_feature("velocity-12")
		.set_file_name("wasm_binary_elastic_scaling_500ms.rs")
		.build();

	// A runtime with a slot duration of 6s but parameters that allow multiple blocks per slot.
	WasmBuilder::init_with_defaults()
		.enable_feature("velocity-6")
		.set_file_name("wasm_binary_elastic_scaling_multi_block_slot.rs")
		.build();

	// A runtime that uses a relay parent offset of 2.
	WasmBuilder::init_with_defaults()
		.enable_feature("relay-parent-offset-2")
		.set_file_name("wasm_binary_relay_parent_offset.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("sync-backing")
		.enable_feature("12s-slot")
		.set_file_name("wasm_binary_sync_backing.rs")
		.build();

	// An elastic scaling runtime with 12s slots.
	WasmBuilder::init_with_defaults()
		.enable_feature("12s-slot")
		.enable_feature("velocity-3")
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_elastic_scaling_12s_slot.rs")
		.build();

	// A runtime that uses block-bundling.
	WasmBuilder::init_with_defaults()
		.enable_feature("velocity-12")
		.set_file_name("wasm_binary_block_bundling.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("v3-descriptor")
		.set_file_name("wasm_binary_async_backing_v3.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("v3-descriptor")
		.enable_feature("relay-parent-offset-2")
		.set_file_name("wasm_binary_async_backing_v3_rpo.rs")
		.build();

	WasmBuilder::init_with_defaults()
		.enable_feature("v3-descriptor")
		.enable_feature("velocity-3")
		.set_file_name("wasm_binary_elastic_scaling_v3.rs")
		.build();

	// A runtime with 18s slot duration with increased spec version for runtime upgrade testing.
	WasmBuilder::init_with_defaults()
		.enable_feature("18s-slot")
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_slot_duration_18s.rs")
		.build();

	WasmBuilder::new()
		.with_current_project()
		.enable_feature("with-authority-discovery")
		.set_file_name("wasm_binary_with_authority_discovery.rs")
		.build();
}

#[cfg(not(feature = "std"))]
fn main() {}
