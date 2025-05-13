// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Helper imports to make it easy to run the AHM integration tests for different runtimes.

#![cfg(test)]

pub mod bench_ah;
pub mod bench_ops;
pub mod bench_rc;
pub mod call_filter_asset_hub;
pub mod call_filter_relay;
pub mod checks;
pub mod mock;
pub mod multisig_still_work;
pub mod proxy;
pub mod tests;

/// Imports for the AHM tests that can be reused for other chains.
pub mod porting_prelude {
	// Dependency renaming depending on runtimes or SDK names:
	#[cfg(feature = "ahm-polkadot")]
	pub mod dependency_alias {
		// Polkadot it is the canonical code
	}
	#[cfg(feature = "ahm-westend")]
	pub mod dependency_alias {
		// Westend lives in the Polkadot SDK - it has different dependency names:
		pub use polkadot_runtime_parachains as runtime_parachains;
		pub use sp_authority_discovery as authority_discovery_primitives;
		pub use sp_consensus_babe as babe_primitives;
		pub use sp_consensus_beefy as beefy_primitives;
		pub use sp_consensus_grandpa as grandpa;
	}
	pub use dependency_alias::*;

	// Import renaming depending on runtimes or SDK names:
	#[cfg(feature = "ahm-polkadot")]
	pub mod import_alias {
		pub use polkadot_runtime_constants::DOLLARS as RC_DOLLARS;
	}
	#[cfg(feature = "ahm-westend")]
	pub mod import_alias {
		pub use asset_hub_westend_runtime as asset_hub_polkadot_runtime;
		pub use westend_runtime as polkadot_runtime;
		pub use westend_runtime_constants as polkadot_runtime_constants;

		pub use testnet_parachains_constants::westend::currency::DOLLARS as RC_DOLLARS;
	}
	pub use import_alias::*;

	// Convenience aliases:
	pub use asset_hub_polkadot_runtime::Runtime as AhRuntime;
	pub use polkadot_runtime::Runtime as RcRuntime;

	// Westend does not support remote proxies, so we have to figure out the import location:
	#[cfg(feature = "ahm-westend")]
	pub use polkadot_runtime as rc_proxy_definition;
	#[cfg(feature = "ahm-polkadot")]
	pub use polkadot_runtime_constants::proxy as rc_proxy_definition;
}

#[doc(hidden)]
mod sanity_checks {
	#[cfg(not(any(feature = "ahm-polkadot", feature = "ahm-westend")))]
	compile_error!("You must enable exactly one of the features: `ahm-polkadot` or `ahm-westend`");
	#[cfg(all(feature = "ahm-polkadot", feature = "ahm-westend"))]
	compile_error!("Cannot enable multiple `ahm-test-*` features at once");
}
