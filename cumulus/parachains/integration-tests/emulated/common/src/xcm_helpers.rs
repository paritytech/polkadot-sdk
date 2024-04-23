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

// Cumulus
use parachains_common::AccountId;

// Polkadot
use xcm::{prelude::*, DoubleEncoded};

/// Helper method to build a XCM with a `Transact` instruction and paying for its execution
pub fn xcm_transact_paid_execution(
	call: DoubleEncoded<()>,
	origin_kind: OriginKind,
	native_asset: Asset,
	beneficiary: AccountId,
) -> VersionedXcm<()> {
	let weight_limit = WeightLimit::Unlimited;
	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	let native_assets: Assets = native_asset.clone().into();

	VersionedXcm::from(Xcm(vec![
		WithdrawAsset(native_assets),
		BuyExecution { fees: native_asset, weight_limit },
		Transact { require_weight_at_most, origin_kind, call },
		RefundSurplus,
		DepositAsset {
			assets: All.into(),
			beneficiary: Location {
				parents: 0,
				interior: [AccountId32 { network: None, id: beneficiary.into() }].into(),
			},
		},
	]))
}

/// Helper method to build a XCM with a `Transact` instruction without paying for its execution
pub fn xcm_transact_unpaid_execution(
	call: DoubleEncoded<()>,
	origin_kind: OriginKind,
) -> VersionedXcm<()> {
	let weight_limit = WeightLimit::Unlimited;
	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	let check_origin = None;

	VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		Transact { require_weight_at_most, origin_kind, call },
	]))
}

/// Helper method to get the non-fee asset used in multiple assets transfer
pub fn non_fee_asset(assets: &Assets, fee_idx: usize) -> Option<(Location, u128)> {
	let asset = assets.inner().into_iter().enumerate().find(|a| a.0 != fee_idx)?.1.clone();
	let asset_amount = match asset.fun {
		Fungible(amount) => amount,
		_ => return None,
	};
	Some((asset.id.0, asset_amount))
}
