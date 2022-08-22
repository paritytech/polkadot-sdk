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
	Weight, WeightToFeeCoefficient, WeightToFeeCoefficients, WeightToFeePolynomial,
};
use sp_runtime::FixedU128;
use sp_std::prelude::*;
use sp_version::RuntimeVersion;

pub use bp_polkadot_core::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, decl_bridge_runtime_apis,
};

/// Rococo Chain
pub type Rococo = PolkadotLike;

/// The target length of a session (how often authorities change) on Rococo measured in of number
/// of blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = time_units::HOURS;

// NOTE: This needs to be kept up to date with the Rococo runtime found in the Polkadot repo.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: sp_version::create_runtime_str!("rococo"),
	impl_name: sp_version::create_runtime_str!("parity-rococo-v2.0"),
	authoring_version: 0,
	spec_version: 9200,
	impl_version: 0,
	apis: sp_version::create_apis_vec![[]],
	transaction_version: 0,
	state_version: 0,
};

// NOTE: This needs to be kept up to date with the Rococo runtime found in the Polkadot repo.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Balance> {
		const CENTS: Balance = 1_000_000_000_000 / 100;
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

/// Name of the With-Rococo GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_ROCOCO_GRANDPA_PALLET_NAME: &str = "BridgeRococoGrandpa";
/// Name of the With-Rococo messages pallet instance that is deployed at bridged chains.
pub const WITH_ROCOCO_MESSAGES_PALLET_NAME: &str = "BridgeRococoMessages";

/// Existential deposit on Rococo.
pub const EXISTENTIAL_DEPOSIT: Balance = 1_000_000_000_000 / 100;

/// Weight of pay-dispatch-fee operation for inbound messages at Rococo chain.
///
/// This value corresponds to the result of
/// `pallet_bridge_messages::WeightInfoExt::pay_inbound_dispatch_fee_overhead()` call for your
/// chain. Don't put too much reserve there, because it is used to **decrease**
/// `DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT` cost. So putting large reserve would make delivery
/// transactions cheaper.
pub const PAY_INBOUND_DISPATCH_FEE_WEIGHT: Weight = 600_000_000;

decl_bridge_runtime_apis!(rococo);
