// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Helpers for calculating XCM delivery fees.

use xcm::latest::prelude::*;

/// Returns the delivery fees amount for pallet xcm's `teleport_assets` extrinsics.
/// Because it returns only a `u128`, it assumes delivery fees are only paid
/// in one asset and that asset is known.
pub fn teleport_assets_delivery_fees<S: SendXcm>(
	assets: Assets,
	fee_asset_item: u32,
	weight_limit: WeightLimit,
	beneficiary: Location,
	destination: Location,
) -> u128 {
	let message = teleport_assets_dummy_message(assets, fee_asset_item, weight_limit, beneficiary);
	get_fungible_delivery_fees::<S>(destination, message)
}

/// Returns the delivery fees amount for a query response as a result of the execution
/// of a `ExpectError` instruction with no error.
pub fn query_response_delivery_fees<S: SendXcm>(querier: Location) -> u128 {
	// Message to calculate delivery fees, it's encoded size is what's important.
	// This message reports that there was no error, if an error is reported, the encoded size would
	// be different.
	let message = Xcm(vec![
		SetFeesMode { jit_withdraw: true },
		QueryResponse {
			query_id: 0, // Dummy query id
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier.clone()),
		},
		SetTopic([0u8; 32]), // Dummy topic
	]);
	get_fungible_delivery_fees::<S>(querier, message)
}

/// Returns the delivery fees amount for the execution of `PayOverXcm`
pub fn pay_over_xcm_delivery_fees<S: SendXcm>(
	interior: Junctions,
	destination: Location,
	beneficiary: Location,
	asset: Asset,
) -> u128 {
	// This is a dummy message.
	// The encoded size is all that matters for delivery fees.
	let message = Xcm(vec![
		DescendOrigin(interior),
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		SetAppendix(Xcm(vec![
			SetFeesMode { jit_withdraw: true },
			ReportError(QueryResponseInfo {
				destination: destination.clone(),
				query_id: 0,
				max_weight: Weight::zero(),
			}),
		])),
		TransferAsset { beneficiary, assets: vec![asset].into() },
	]);
	get_fungible_delivery_fees::<S>(destination, message)
}

/// Approximates the actual message sent by the teleport extrinsic.
/// The assets are not reanchored and the topic is a dummy one.
/// However, it should have the same encoded size, which is what matters for delivery fees.
/// Also has same encoded size as the one created by the reserve transfer assets extrinsic.
fn teleport_assets_dummy_message(
	assets: Assets,
	fee_asset_item: u32,
	weight_limit: WeightLimit,
	beneficiary: Location,
) -> Xcm<()> {
	Xcm(vec![
		ReceiveTeleportedAsset(assets.clone()), // Same encoded size as `ReserveAssetDeposited`
		ClearOrigin,
		BuyExecution { fees: assets.get(fee_asset_item as usize).unwrap().clone(), weight_limit },
		DepositAsset { assets: Wild(AllCounted(assets.len() as u32)), beneficiary },
		SetTopic([0u8; 32]), // Dummy topic
	])
}

/// Given a message, a sender, and a destination, it returns the delivery fees
fn get_fungible_delivery_fees<S: SendXcm>(destination: Location, message: Xcm<()>) -> u128 {
	let delivery_fees = match validate_send::<S>(destination, message) {
		Ok((_, delivery_fees)) => delivery_fees,
		Err(e) => unreachable!("message can be sent - {:?}; qed", e),
	};
	if let Some(delivery_fee) = delivery_fees.inner().first() {
		let Fungible(delivery_fee_amount) = delivery_fee.fun else {
			unreachable!("asset is fungible; qed");
		};
		delivery_fee_amount
	} else {
		0
	}
}
