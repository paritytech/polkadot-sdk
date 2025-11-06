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
use sp_core::H256;
use xcm::{prelude::*, DoubleEncoded};
use xcm_emulator::Chain;

use crate::impls::{bx, Encode};
use frame_support::dispatch::{DispatchResultWithPostInfo, PostDispatchInfo};
use sp_runtime::traits::{Dispatchable, Hash};
use xcm::{VersionedLocation, VersionedXcm};

/// Helper method to build a XCM with a `Transact` instruction and paying for its execution
pub fn xcm_transact_paid_execution(
	call: DoubleEncoded<()>,
	origin_kind: OriginKind,
	fees: Asset,
	beneficiary: AccountId,
) -> VersionedXcm<()> {
	let weight_limit = WeightLimit::Unlimited;

	VersionedXcm::from(Xcm(vec![
		WithdrawAsset(fees.clone().into()),
		BuyExecution { fees, weight_limit },
		Transact { origin_kind, call, fallback_max_weight: None },
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
	let check_origin = None;

	VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		Transact { origin_kind, call, fallback_max_weight: None },
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

/// Helper method to get the fee asset used in multiple assets transfer
pub fn fee_asset(assets: &Assets, fee_idx: usize) -> Option<(Location, u128)> {
	let asset = assets.get(fee_idx)?;
	let asset_amount = match asset.fun {
		Fungible(amount) => amount,
		_ => return None,
	};
	Some((asset.id.0.clone(), asset_amount))
}

pub fn get_amount_from_versioned_assets(assets: VersionedAssets) -> u128 {
	let latest_assets: Assets = assets.try_into().unwrap();
	let Fungible(amount) = latest_assets.inner()[0].fun else {
		unreachable!("asset is non-fungible");
	};
	amount
}

fn to_mq_processed_id<C: Chain>(event: C::RuntimeEvent) -> Option<H256>
where
	<C as Chain>::Runtime: pallet_message_queue::Config,
	C::RuntimeEvent: TryInto<pallet_message_queue::Event<<C as Chain>::Runtime>>,
{
	if let Ok(pallet_message_queue::Event::Processed { id, .. }) = event.try_into() {
		Some(id)
	} else {
		None
	}
}

/// Helper method to find all `Event::Processed` IDs from the chain's events.
pub fn find_all_mq_processed_ids<C: Chain>() -> Vec<H256>
where
	<C as Chain>::Runtime: pallet_message_queue::Config,
	C::RuntimeEvent: TryInto<pallet_message_queue::Event<<C as Chain>::Runtime>>,
{
	C::events().into_iter().filter_map(to_mq_processed_id::<C>).collect()
}

/// Helper method to find the ID of the first `Event::Processed` event in the chain's events.
pub fn find_mq_processed_id<C: Chain>() -> Option<H256>
where
	<C as Chain>::Runtime: pallet_message_queue::Config,
	C::RuntimeEvent: TryInto<pallet_message_queue::Event<<C as Chain>::Runtime>>,
{
	C::events().into_iter().find_map(to_mq_processed_id::<C>)
}

/// Helper method to find the message ID of the first `Event::Sent` event in the chain's events.
pub fn find_xcm_sent_message_id<
	C: Chain<RuntimeEvent = <<C as Chain>::Runtime as pallet_xcm::Config>::RuntimeEvent>,
>() -> Option<XcmHash>
where
	C::Runtime: pallet_xcm::Config,
	C::RuntimeEvent: TryInto<pallet_xcm::Event<C::Runtime>>,
{
	pallet_xcm::xcm_helpers::find_xcm_sent_message_id::<<C as Chain>::Runtime>(C::events())
}

/// Wraps a runtime call in a whitelist preimage call and dispatches it
pub fn dispatch_whitelisted_call_with_preimage<T>(
	call: T::RuntimeCall,
	origin: T::RuntimeOrigin,
) -> DispatchResultWithPostInfo
where
	T: Chain,
	T::Runtime: pallet_whitelist::Config,
	T::RuntimeCall: From<pallet_whitelist::Call<T::Runtime>>
		+ Into<<T::Runtime as pallet_whitelist::Config>::RuntimeCall>
		+ Dispatchable<RuntimeOrigin = T::RuntimeOrigin, PostInfo = PostDispatchInfo>,
{
	T::execute_with(|| {
		let whitelist_call: T::RuntimeCall =
			pallet_whitelist::Call::<T::Runtime>::dispatch_whitelisted_call_with_preimage {
				call: Box::new(call.into()),
			}
			.into();
		whitelist_call.dispatch(origin)
	})
}

/// Builds a `pallet_xcm::send` call to authorize an upgrade at the provided location,
/// wrapped in an unpaid XCM `Transact` with `OriginKind::Superuser`.
pub fn build_xcm_send_authorize_upgrade_call<T, D>(
	location: Location,
	code_hash: &H256,
	fallback_max_weight: Option<Weight>,
) -> T::RuntimeCall
where
	T: Chain,
	T::Runtime: pallet_xcm::Config,
	T::RuntimeCall: Encode + From<pallet_xcm::Call<T::Runtime>>,
	D: Chain,
	D::Runtime: frame_system::Config<Hash = H256>,
	D::RuntimeCall: Encode + From<frame_system::Call<D::Runtime>>,
{
	let transact_call: D::RuntimeCall =
		frame_system::Call::authorize_upgrade { code_hash: *code_hash }.into();

	let call: T::RuntimeCall = pallet_xcm::Call::send {
		dest: bx!(VersionedLocation::from(location)),
		message: bx!(VersionedXcm::from(Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Superuser,
				fallback_max_weight,
				call: transact_call.encode().into(),
			}
		]))),
	}
	.into();
	call
}

/// Encodes a runtime call and returns its H256 hash
pub fn call_hash_of<T>(call: &T::RuntimeCall) -> H256
where
	T: Chain,
	T::Runtime: frame_system::Config<Hash = H256>,
	T::RuntimeCall: Encode,
{
	<T::Runtime as frame_system::Config>::Hashing::hash_of(&call)
}
