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
//! Three WASM blobs are produced:
//!
//! 1. The default runtime ([`crate::WASM_BINARY`]).
//! 2. A spec-version-bumped variant used by runtime-upgrade tests. Behaviour-identical to the
//!    default WASM except `spec_version` is increased by one, so a `set_code` upgrade is
//!    observable on-chain.
//! 3. A structural `with-authority-discovery` variant that wires in `pallet_session` +
//!    `pallet_authority_discovery` and runs an on-chain migration. This is the only variant
//!    that genuinely needs its own WASM (different runtime topology).
//!
//! All other consensus parameters — slot duration, block-processing velocity, relay-parent
//! offset, allow-multiple-blocks-per-slot, scheduling V3 toggle — are runtime-configurable
//! via `pallet_parameters` and seeded by named GenesisBuilder presets, so they no longer
//! require their own WASM artifacts.

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

	// 3. Structural variant: adds `pallet_session` + `pallet_authority_discovery` and runs the
	// `EnableAuthorityDiscovery` migration on upgrade.
	WasmBuilder::new()
		.with_current_project()
		.enable_feature("with-authority-discovery")
		.set_file_name("wasm_binary_with_authority_discovery.rs")
		.build();
}

#[cfg(not(feature = "std"))]
fn main() {}
