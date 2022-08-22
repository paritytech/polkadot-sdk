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

/// Polkadot Chain
pub type Polkadot = PolkadotLike;

// NOTE: This needs to be kept up to date with the Polkadot runtime found in the Polkadot repo.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: sp_version::create_runtime_str!("polkadot"),
	impl_name: sp_version::create_runtime_str!("parity-polkadot"),
	authoring_version: 0,
	spec_version: 9180,
	impl_version: 0,
	apis: sp_version::create_apis_vec![[]],
	transaction_version: 12,
	state_version: 0,
};

// NOTE: This needs to be kept up to date with the Polkadot runtime found in the Polkadot repo.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		const CENTS: Balance = 10_000_000_000 / 100;
		// in Polkadot, extrinsic base weight (smallest non-zero weight) is mapped to 1/10 CENT:
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

/// Per-byte fee for Polkadot transactions.
pub const TRANSACTION_BYTE_FEE: Balance = 10 * 10_000_000_000 / 100 / 1_000;

/// Existential deposit on Polkadot.
pub const EXISTENTIAL_DEPOSIT: Balance = 10_000_000_000;

/// The target length of a session (how often authorities change) on Polkadot measured in of number
/// of blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 4 * time_units::HOURS;

/// Name of the With-Polkadot GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_POLKADOT_GRANDPA_PALLET_NAME: &str = "BridgePolkadotGrandpa";
/// Name of the With-Polkadot messages pallet instance that is deployed at bridged chains.
pub const WITH_POLKADOT_MESSAGES_PALLET_NAME: &str = "BridgePolkadotMessages";

/// Name of the transaction payment pallet at the Polkadot runtime.
pub const TRANSACTION_PAYMENT_PALLET_NAME: &str = "TransactionPayment";

/// Name of the KSM->DOT conversion rate parameter, stored in the Polkadot runtime.
pub const KUSAMA_TO_POLKADOT_CONVERSION_RATE_PARAMETER_NAME: &str =
	"KusamaToPolkadotConversionRate";
/// Name of the Kusama fee multiplier parameter, stored in the Polkadot runtime.
pub const KUSAMA_FEE_MULTIPLIER_PARAMETER_NAME: &str = "KusamaFeeMultiplier";

decl_bridge_runtime_apis!(polkadot);
