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

use bp_messages::{
	InboundMessageDetails, LaneId, MessageNonce, MessagePayload, OutboundMessageDetails,
};
use frame_support::weights::{
	WeightToFeeCoefficient, WeightToFeeCoefficients, WeightToFeePolynomial,
};
use sp_runtime::FixedU128;
use sp_std::prelude::*;
use sp_version::RuntimeVersion;

pub use bp_polkadot_core::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, decl_bridge_runtime_apis,
};

/// Kusama Chain
pub type Kusama = PolkadotLike;

// NOTE: This needs to be kept up to date with the Kusama runtime found in the Polkadot repo.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: sp_version::create_runtime_str!("kusama"),
	impl_name: sp_version::create_runtime_str!("parity-kusama"),
	authoring_version: 2,
	spec_version: 9180,
	impl_version: 0,
	apis: sp_version::create_apis_vec![[]],
	transaction_version: 11,
	state_version: 0,
};

// NOTE: This needs to be kept up to date with the Kusama runtime found in the Polkadot repo.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		const CENTS: Balance = 1_000_000_000_000 / 30_000;
		// in Kusama, extrinsic base weight (smallest non-zero weight) is mapped to 1/10 CENT:
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

/// Per-byte fee for Kusama transactions.
pub const TRANSACTION_BYTE_FEE: Balance = 10 * 1_000_000_000_000 / 30_000 / 1_000;

/// Existential deposit on Kusama.
pub const EXISTENTIAL_DEPOSIT: Balance = 1_000_000_000_000 / 30_000;

/// The target length of a session (how often authorities change) on Kusama measured in of number of
/// blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = time_units::HOURS;

/// Name of the With-Kusama GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_KUSAMA_GRANDPA_PALLET_NAME: &str = "BridgeKusamaGrandpa";
/// Name of the With-Kusama messages pallet instance that is deployed at bridged chains.
pub const WITH_KUSAMA_MESSAGES_PALLET_NAME: &str = "BridgeKusamaMessages";

/// Name of the transaction payment pallet at the Kusama runtime.
pub const TRANSACTION_PAYMENT_PALLET_NAME: &str = "TransactionPayment";

/// Name of the DOT->KSM conversion rate stored in the Kusama runtime.
pub const POLKADOT_TO_KUSAMA_CONVERSION_RATE_PARAMETER_NAME: &str =
	"PolkadotToKusamaConversionRate";
/// Name of the Polkadot fee multiplier parameter, stored in the Polkadot runtime.
pub const POLKADOT_FEE_MULTIPLIER_PARAMETER_NAME: &str = "PolkadotFeeMultiplier";

decl_bridge_runtime_apis!(kusama);
