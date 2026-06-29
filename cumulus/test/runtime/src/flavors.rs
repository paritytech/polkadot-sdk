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

//! Additional WASM blob accessors built by `build.rs`.
//!
//! Consensus tuning variants moved to runtime parameters; only upgrade-test blobs remain here.

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
