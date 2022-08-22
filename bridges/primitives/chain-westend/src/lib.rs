// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]

use frame_support::weights::{
	WeightToFeeCoefficient, WeightToFeeCoefficients, WeightToFeePolynomial,
};
use scale_info::TypeInfo;
use sp_std::prelude::*;
use sp_version::RuntimeVersion;

pub use bp_polkadot_core::*;
use bp_runtime::decl_bridge_finality_runtime_apis;

/// Westend Chain
pub type Westend = PolkadotLike;

// NOTE: This needs to be kept up to date with the Westend runtime found in the Polkadot repo.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		const CENTS: Balance = 1_000_000_000_000 / 1_000;
		// in Westend, extrinsic base weight (smallest non-zero weight) is mapped to 1/10 CENT:
		let p = CENTS;
		let q = 10 * Balance::from(ExtrinsicBaseWeight::get());
		smallvec::smallvec![WeightToFeeCoefficient {
			degree: 1,
			negative: false,
			coeff_frac: Perbill::from_rational(p % q, q),
			coeff_integer: p / q,
		}]
	}
}

// NOTE: This needs to be kept up to date with the Westend runtime found in the Polkadot repo.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: sp_version::create_runtime_str!("westend"),
	impl_name: sp_version::create_runtime_str!("parity-westend"),
	authoring_version: 2,
	spec_version: 9140,
	impl_version: 0,
	apis: sp_version::create_apis_vec![[]],
	transaction_version: 8,
	state_version: 0,
};

/// Westend Runtime `Call` enum.
///
/// We are not currently submitting any Westend transactions => it is empty.
#[derive(codec::Encode, codec::Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {}

impl sp_runtime::traits::Dispatchable for Call {
	type Origin = ();
	type Config = ();
	type Info = ();
	type PostInfo = ();

	fn dispatch(self, _origin: Self::Origin) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
		unimplemented!("The Call is not expected to be dispatched.")
	}
}

/// Name of the parachains pallet at the Westend runtime.
pub const PARAS_PALLET_NAME: &str = "Paras";

/// Name of the With-Westend GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_WESTEND_GRANDPA_PALLET_NAME: &str = "BridgeWestendGrandpa";
/// Name of the With-Westend parachains bridge pallet instance that is deployed at bridged chains.
pub const WITH_WESTEND_BRIDGE_PARAS_PALLET_NAME: &str = "BridgeWestendParachains";

/// The target length of a session (how often authorities change) on Westend measured in of number
/// of blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 10 * time_units::MINUTES;

/// Identifier of Westmint parachain at the Westend relay chain.
pub const WESTMINT_PARACHAIN_ID: u32 = 2000;

decl_bridge_finality_runtime_apis!(westend);

decl_bridge_finality_runtime_apis!(westmint);
