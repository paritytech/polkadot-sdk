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

//! WASM blob accessors for the additional variants built by `build.rs`.
//!
//! Most consensus parameters (slot duration, block-processing velocity, relay-parent offset,
//! `AllowMultipleBlocksPerSlot`, scheduling V3) are now runtime-configurable via
//! `pallet_parameters` and seeded by named GenesisBuilder presets — they no longer require
//! distinct WASM artifacts. Only two extra WASM blobs are produced beyond the default
//! [`crate::WASM_BINARY`]:
//!
//! - [`spec_version_incremented`] — behaviour-identical to the default WASM but with
//!   `spec_version` bumped by one. Used by runtime-upgrade tests that need to observe a
//!   `set_code` transition.
//! - [`with_authority_discovery`] — structural variant adding `pallet_session` +
//!   `pallet_authority_discovery`. Used by the authority-discovery upgrade test.

/// Same runtime as [`crate::WASM_BINARY`] but with `spec_version` bumped by one — used by
/// runtime-upgrade tests to observe a `set_code` transition.
pub mod spec_version_incremented {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_spec_version_incremented.rs"));
}

/// Structural runtime variant: adds `pallet_session` + `pallet_authority_discovery` and runs
/// the `EnableAuthorityDiscovery` migration on upgrade. Used by the authority-discovery
/// upgrade test.
pub mod with_authority_discovery {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_with_authority_discovery.rs"));
}
