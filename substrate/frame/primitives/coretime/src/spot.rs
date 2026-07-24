// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The adaptive on-demand coretime spot price calculation.
//!
//! Shared between the Relay-chain's on-demand pallet and `pallet-broker`, so that orders placed
//! on either chain are priced by the same mechanism.

use sp_runtime::{traits::One, FixedU128, Perbill, Saturating};

/// Errors that can happen during spot traffic calculation.
#[derive(PartialEq, Debug)]
pub enum SpotTrafficCalculationErr {
	/// The order queue capacity is at 0.
	QueueCapacityIsZero,
	/// The queue size is larger than the queue capacity.
	QueueSizeLargerThanCapacity,
	/// Arithmetic error during division, either division by 0 or over/underflow.
	Division,
}

/// The spot price multiplier. This is based on the transaction fee calculations defined in:
/// https://research.web3.foundation/Polkadot/overview/token-economics#setting-transaction-fees
///
/// Parameters:
/// - `traffic`: The previously calculated multiplier, can never go below `traffic_floor`.
/// - `queue_capacity`: The max size of the order book.
/// - `queue_size`: How many orders are currently in the order book.
/// - `target_queue_utilisation`: How much of the queue_capacity should be ideally occupied,
///   expressed in percentages(perbill).
/// - `variability`: A variability factor, i.e. how quickly the spot price adjusts. This number can
///   be chosen by p/(k*(1-s)) where p is the desired ratio increase in spot price over k number of
///   blocks. s is the target_queue_utilisation. A concrete example: v = 0.05/(20*(1-0.25)) =
///   0.0033.
/// - `traffic_floor`: The minimum multiplier. The spot price can never fall below `traffic_floor *
///   base_fee`.
///
/// Returns:
/// - A `FixedU128` in the range of `traffic_floor` - `FixedU128::MAX` on success.
///
/// Errors:
/// - `SpotTrafficCalculationErr::QueueCapacityIsZero`
/// - `SpotTrafficCalculationErr::QueueSizeLargerThanCapacity`
/// - `SpotTrafficCalculationErr::Division`
pub fn calculate_spot_traffic(
	traffic: FixedU128,
	queue_capacity: u32,
	queue_size: u32,
	target_queue_utilisation: Perbill,
	variability: Perbill,
	traffic_floor: FixedU128,
) -> Result<FixedU128, SpotTrafficCalculationErr> {
	// Return early if queue has no capacity.
	if queue_capacity == 0 {
		return Err(SpotTrafficCalculationErr::QueueCapacityIsZero);
	}

	// Return early if queue size is greater than capacity.
	if queue_size > queue_capacity {
		return Err(SpotTrafficCalculationErr::QueueSizeLargerThanCapacity);
	}

	// (queue_size / queue_capacity) - target_queue_utilisation
	let queue_util_ratio = FixedU128::from_rational(queue_size.into(), queue_capacity.into());
	let positive = queue_util_ratio >= target_queue_utilisation.into();
	let queue_util_diff = queue_util_ratio.max(target_queue_utilisation.into()) -
		queue_util_ratio.min(target_queue_utilisation.into());

	// variability * queue_util_diff
	let var_times_qud = queue_util_diff.saturating_mul(variability.into());

	// variability^2 * queue_util_diff^2
	let var_times_qud_pow = var_times_qud.saturating_mul(var_times_qud);

	// (variability^2 * queue_util_diff^2)/2
	let div_by_two: FixedU128;
	match var_times_qud_pow.const_checked_div(2.into()) {
		Some(dbt) => div_by_two = dbt,
		None => return Err(SpotTrafficCalculationErr::Division),
	}

	// traffic * (1 + queue_util_diff) + div_by_two
	if positive {
		let new_traffic = queue_util_diff
			.saturating_add(div_by_two)
			.saturating_add(One::one())
			.saturating_mul(traffic);
		Ok(new_traffic.max(traffic_floor))
	} else {
		let new_traffic = queue_util_diff.saturating_sub(div_by_two).saturating_mul(traffic);
		Ok(new_traffic.max(traffic_floor))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const FLOOR: FixedU128 = FixedU128::from_u32(1);

	#[test]
	fn calculate_spot_traffic_zero_capacity_errors() {
		assert_eq!(
			calculate_spot_traffic(
				FixedU128::from_u32(1),
				0,
				100,
				Perbill::from_percent(25),
				Perbill::from_percent(3),
				FLOOR,
			),
			Err(SpotTrafficCalculationErr::QueueCapacityIsZero)
		);
	}

	#[test]
	fn calculate_spot_traffic_queue_size_larger_than_capacity_errors() {
		assert_eq!(
			calculate_spot_traffic(
				FixedU128::from_u32(1),
				100,
				101,
				Perbill::from_percent(25),
				Perbill::from_percent(3),
				FLOOR,
			),
			Err(SpotTrafficCalculationErr::QueueSizeLargerThanCapacity)
		);
	}

	#[test]
	fn calculate_spot_traffic_identity_at_target_utilisation() {
		// Queue utilisation exactly at target: the multiplier stays put.
		assert_eq!(
			calculate_spot_traffic(
				FixedU128::from_u32(1),
				100,
				25,
				Perbill::from_percent(25),
				Perbill::from_percent(3),
				FLOOR,
			),
			Ok(FixedU128::from_u32(1))
		);
	}

	#[test]
	fn calculate_spot_traffic_increases_above_target() {
		let traffic = calculate_spot_traffic(
			FixedU128::from_u32(1),
			100,
			100,
			Perbill::from_percent(25),
			Perbill::from_percent(3),
			FLOOR,
		)
		.expect("valid inputs; qed");
		assert!(traffic > FixedU128::from_u32(1));

		// Sustained above-target utilisation keeps increasing the multiplier.
		let higher = calculate_spot_traffic(
			traffic,
			100,
			100,
			Perbill::from_percent(25),
			Perbill::from_percent(3),
			FLOOR,
		)
		.expect("valid inputs; qed");
		assert!(higher > traffic);
	}

	#[test]
	fn calculate_spot_traffic_decreases_below_target_and_clamps_to_floor() {
		let elevated = FixedU128::from_rational(101, 100);
		let decreased = calculate_spot_traffic(
			elevated,
			100,
			0,
			Perbill::from_percent(25),
			Perbill::from_percent(3),
			FLOOR,
		)
		.expect("valid inputs; qed");
		assert!(decreased < elevated);

		// Never falls below the floor.
		let mut traffic = decreased;
		for _ in 0..100 {
			traffic = calculate_spot_traffic(
				traffic,
				100,
				0,
				Perbill::from_percent(25),
				Perbill::from_percent(3),
				FLOOR,
			)
			.expect("valid inputs; qed");
		}
		assert_eq!(traffic, FLOOR);
	}
}
