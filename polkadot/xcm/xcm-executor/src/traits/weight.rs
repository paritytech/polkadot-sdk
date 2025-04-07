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

use core::result::Result;
use xcm::latest::{prelude::*, Weight};

/// Determine the weight of an XCM message.
pub trait WeightBounds<RuntimeCall> {
	/// Return the maximum amount of weight that an attempted execution of this message could
	/// consume.
	fn weight(message: &mut Xcm<RuntimeCall>) -> Result<Weight, ()>;

	/// Return the maximum amount of weight that an attempted execution of this instruction could
	/// consume.
	fn instr_weight(instruction: &mut Instruction<RuntimeCall>) -> Result<Weight, ()>;
}

#[derive(Debug, Clone,PartialEq, Eq)]
pub enum WeightFee {
	Desired(u128),
	Swap {
		required_fee: (AssetId, u128),
		swap_amount: u128,
	}
}

// FIXME docs
/// Get the weight price in order to buy it to execute XCM.
///
/// A `WeightTrader` may also be put into a tuple, in which case the default behavior of
/// `weight_price` would be to attempt to call each tuple element's own
/// implementation of these two functions, in the order of which they appear in the tuple,
/// returning early when a successful result is returned.
pub trait WeightTrader {
	fn weight_fee(weight: &Weight, desired_asset_id: &AssetId, context: Option<&XcmContext>) -> Result<WeightFee, XcmError>;

	fn refund_amount(weight: &Weight, used_asset_id: &AssetId, paid_amount: u128, context: Option<&XcmContext>) -> Option<u128> {
		Self::weight_fee(weight, used_asset_id, context)
			.ok()
			.map(|wf| if let WeightFee::Desired(amount) = wf { Some(amount) } else { None })
			.flatten()
	}

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl WeightTrader for Tuple {
	fn weight_fee(weight: &Weight, desired_asset_id: &AssetId, context: Option<&XcmContext>) -> Result<WeightFee, XcmError> {
		for_tuples!( #(
			let weight_trader = core::any::type_name::<Tuple>();
			
			match Tuple::weight_fee(weight, desired_asset_id, context) {
				Ok(fee) => {
					tracing::trace!(
						target: "xcm::weight_trader", 
						%weight_trader,
						"Getting weight price succeeded",
					);

					return Ok(fee);
				},
				Err(error) => {
					tracing::trace!(
						target: "xcm::weight_trader", 
						?error,
						%weight_trader,
						"Getting weight price failed",
					);
				}
			}
		)* );

		tracing::trace!(
			target: "xcm::weight_trader",
			"Getting weight price failed",
		);

		Err(XcmError::TooExpensive)
	}

	fn refund_amount(weight: &Weight, used_asset_id: &AssetId, paid_amount: u128, context: Option<&XcmContext>) -> Option<u128> {
		for_tuples!( #(
			let weight_trader = core::any::type_name::<Tuple>();
			
			match Tuple::refund_amount(weight, used_asset_id, paid_amount, context) {
				Some(refund_amount) => {
					tracing::trace!(
						target: "xcm::weight_trader", 
						%weight_trader,
						"Getting refund amount succeeded",
					);

					return Some(refund_amount);
				},
				None => {
					tracing::trace!(
						target: "xcm::weight_trader", 
						%weight_trader,
						"Getting refund amount failed",
					);
				}
			}
		)* );

		tracing::trace!(
			target: "xcm::weight_trader",
			"Getting refund amount failed",
		);

		None
	}

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool {
		for_tuples!( #(
			let weight_trader = core::any::type_name::<Tuple>();

			if Tuple::take_fee(asset_id, amount) {
				tracing::trace!(
					target: "xcm::weight_trader", 
					%weight_trader,
					"Asset is taken",
				);
				return true;
			} else {
				tracing::trace!(
					target: "xcm::weight_trader", 
					%weight_trader,
					"Asset is skipped",
				);
			}
		)* );

		tracing::trace!(
			target: "xcm::weight_trader", 
			"All assets are skipped",
		);

		false
	}
}

// FIXME docs
/// Must not be used in production
pub mod testing {
	use super::*;

	pub trait TraderTest {
		fn test_buy_weight(&mut self, weight: Weight, max_payment: Asset) -> Result<(AssetId, u128), XcmError>;

		fn test_refund_weight(&mut self, weight: Weight) -> Option<(AssetId, u128)>;

		fn test_take_fee(self);
	}
}
