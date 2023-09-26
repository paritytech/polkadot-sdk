// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use parachains_common::AccountId;
use xcm::{
	prelude::*,
	DoubleEncoded,
};
use polkadot_runtime_common::xcm_sender::PriceForParachainDelivery;
use xcm_emulator::TestArgs;

/// Helper method to build a XCM with a `Transact` instruction and paying for its execution
pub fn xcm_transact_paid_execution(
	call: DoubleEncoded<()>,
	origin_kind: OriginKind,
	native_asset: MultiAsset,
	beneficiary: AccountId,
) -> VersionedXcm<()> {
	let weight_limit = WeightLimit::Unlimited;
	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	let native_assets: MultiAssets = native_asset.clone().into();

	VersionedXcm::from(Xcm(vec![
		WithdrawAsset(native_assets),
		BuyExecution { fees: native_asset, weight_limit },
		Transact { require_weight_at_most, origin_kind, call },
		RefundSurplus,
		DepositAsset {
			assets: All.into(),
			beneficiary: MultiLocation {
				parents: 0,
				interior: X1(AccountId32 { network: None, id: beneficiary.into() }),
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

/// Returns the delivery fees amount for pallet xcm's `teleport_assets` and `reserve_transfer_assets` extrinsics.
pub fn transfer_assets_delivery_fees<P: PriceForParachainDelivery>(
	test_args: TestArgs,
) -> u128 {
	// Approximation of the actual message sent by the extrinsic.
	// The assets are not reanchored and the topic is a dummy one.
	// However, it should have the same encoded size, which is what matters for delivery fees.
	let message = Xcm(vec![
		ReceiveTeleportedAsset(test_args.assets.clone()), // Same encoded size as `ReserveAssetDeposited`
		ClearOrigin,
		BuyExecution { fees: test_args.assets.get(test_args.fee_asset_item as usize).unwrap().clone(), weight_limit: test_args.weight_limit },
		DepositAsset { assets: Wild(AllCounted(test_args.assets.len() as u32)), beneficiary: test_args.beneficiary },
		SetTopic([0u8; 32]), // Dummy topic
	]);
	let Parachain(para_id) = test_args.dest.interior().last().unwrap() else { unreachable!("Location is parachain") };
	let delivery_fees = P::price_for_parachain_delivery((*para_id).into(), &message);
	let Fungible(delivery_fees_amount) = delivery_fees.inner()[0].fun else { unreachable!("Asset is fungible") };
	delivery_fees_amount
}
