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

//! Build script for `cumulus-test-runtime`.
//!
//! Five WASM blobs are produced:
//!
//! 1. The default runtime ([`crate::WASM_BINARY`]).
//! 2. `spec_version_incremented` — behaviour-identical to the default WASM except
//!    `spec_version` is bumped by one. Used by runtime-upgrade tests that need to observe a
//!    `set_code` transition without any other behavioural change.
//! 3. `slot_duration_18s` — bumped `spec_version` plus an `OnRuntimeUpgrade` migration that
//!    writes `SlotDurationMillis = 18000` into `pallet_parameters::Parameters`. Used by the
//!    zombienet `parachain_runtime_upgrade_slot_duration_18s` test.
//! 4. `elastic_scaling` — bumped `spec_version` plus an `OnRuntimeUpgrade` migration that
//!    writes `BlockProcessingVelocity = 3`. Used by the zombienet `upgrade_to_3_cores` test
//!    (both async-backing and sync-backing cases — only velocity needs to change; slot
//!    duration is already correct from the starting chain-spec preset).
//! 5. `with_authority_discovery` — structural variant adding `pallet_session` +
//!    `pallet_authority_discovery`, with the `EnableAuthorityDiscovery` migration.
//!
//! All other consensus parameters — slot duration, block-processing velocity, relay-parent
//! offset, allow-multiple-blocks-per-slot, scheduling V3 toggle — are runtime-configurable
//! via `pallet_parameters` and seeded by named GenesisBuilder presets at genesis time, so
//! they no longer require their own WASM artifacts. Variants 3 and 4 exist only to lock in
//! a parameter change *via runtime upgrade*, which is the property those zombienet tests
//! assert on.

#[cfg(feature = "std")]
fn main() {
	use substrate_wasm_builder::WasmBuilder;

	// 1. Default runtime — used by every consensus chain-spec preset.
	WasmBuilder::init_with_defaults().build();

	// 2. Spec-version-bumped variant for `set_code` runtime-upgrade tests.
	WasmBuilder::init_with_defaults()
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_spec_version_incremented.rs")
		.build();

	// 3. Upgrade-target: writes `SlotDurationMillis = 18000` via OnRuntimeUpgrade.
	WasmBuilder::init_with_defaults()
		.enable_feature("slot-duration-18s")
		.set_file_name("wasm_binary_slot_duration_18s.rs")
		.build();

	// 4. Upgrade-target: writes `BlockProcessingVelocity = 3` via OnRuntimeUpgrade.
	WasmBuilder::init_with_defaults()
		.enable_feature("elastic-scaling")
		.set_file_name("wasm_binary_elastic_scaling.rs")
		.build();

	// 5. Structural variant: adds `pallet_session` + `pallet_authority_discovery` and runs the
	// `EnableAuthorityDiscovery` migration on upgrade.
	WasmBuilder::new()
		.with_current_project()
		.enable_feature("with-authority-discovery")
		.set_file_name("wasm_binary_with_authority_discovery.rs")
		.build();
}

#[cfg(not(feature = "std"))]
fn main() {}
