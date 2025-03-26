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

// FIXME docs
/// Get the weight price in order to buy it to execute XCM.
///
/// A `WeightTrader` may also be put into a tuple, in which case the default behavior of
/// `weight_price` would be to attempt to call each tuple element's own
/// implementation of these two functions, in the order of which they appear in the tuple,
/// returning early when a successful result is returned.
pub trait WeightTrader {
	fn weight_price(weight: &Weight, asset_id: &AssetId, context: Option<&XcmContext>) -> Result<(AssetId, u128), XcmError>;

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl WeightTrader for Tuple {
	fn weight_price(weight: &Weight, asset_id: &AssetId, context: Option<&XcmContext>) -> Result<(AssetId, u128), XcmError> {
		for_tuples!( #(
			let weight_trader = core::any::type_name::<Tuple>();
			
			match Tuple::weight_price(weight, asset_id, context) {
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
