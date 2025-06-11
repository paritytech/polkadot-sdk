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

//! # Tests for the Rococo runtime.

use super::*;
use crate::{CENTS, MILLICENTS};
use sp_runtime::traits::Zero;
use sp_weights::WeightToFee;
use testnet_parachains_constants::rococo::fee;

/// We can fit at least 1000 transfers in a block.
#[test]
fn sane_block_weight() {
	use pallet_balances::WeightInfo;
	let block = RuntimeBlockWeights::get().max_block;
	let base = RuntimeBlockWeights::get().get(DispatchClass::Normal).base_extrinsic;
	let transfer = base + weights::pallet_balances::WeightInfo::<Runtime>::transfer_allow_death();

	let fit = block.checked_div_per_component(&transfer).unwrap_or_default();
	assert!(fit >= 1000, "{} should be at least 1000", fit);
}

/// The fee for one transfer is at most 1 CENT.
#[test]
fn sane_transfer_fee() {
	use pallet_balances::WeightInfo;
	let base = RuntimeBlockWeights::get().get(DispatchClass::Normal).base_extrinsic;
	let transfer = base + weights::pallet_balances::WeightInfo::<Runtime>::transfer_allow_death();

	let fee: Balance = fee::WeightToFee::weight_to_fee(&transfer);
	assert!(fee <= CENTS, "{} MILLICENTS should be at most 1000", fee / MILLICENTS);
}

/// Weight is being charged for both dimensions.
#[test]
fn weight_charged_for_both_components() {
	let fee: Balance = fee::WeightToFee::weight_to_fee(&Weight::from_parts(10_000, 0));
	assert!(!fee.is_zero(), "Charges for ref time");

	let fee: Balance = fee::WeightToFee::weight_to_fee(&Weight::from_parts(0, 10_000));
	assert_eq!(fee, CENTS, "10kb maps to CENT");
}

/// Filling up a block by proof size is at most 30 times more expensive than ref time.
///
/// This is just a sanity check.
#[test]
fn full_block_fee_ratio() {
	let block = RuntimeBlockWeights::get().max_block;
	let time_fee: Balance =
		fee::WeightToFee::weight_to_fee(&Weight::from_parts(block.ref_time(), 0));
	let proof_fee: Balance =
		fee::WeightToFee::weight_to_fee(&Weight::from_parts(0, block.proof_size()));

	let proof_o_time = proof_fee.checked_div(time_fee).unwrap_or_default();
	assert!(proof_o_time <= 30, "{} should be at most 30", proof_o_time);
	let time_o_proof = time_fee.checked_div(proof_fee).unwrap_or_default();
	assert!(time_o_proof <= 30, "{} should be at most 30", time_o_proof);
}
