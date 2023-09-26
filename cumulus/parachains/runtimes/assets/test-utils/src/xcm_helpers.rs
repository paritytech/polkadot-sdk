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

use polkadot_runtime_common::xcm_sender::PriceForParachainDelivery;
use xcm::latest::prelude::*;

/// Returns the delivery fees amount for pallet xcm's `teleport_assets` and
/// `reserve_transfer_assets` extrinsics.
pub fn transfer_assets_delivery_fees<P: PriceForParachainDelivery>(
	assets: MultiAssets,
	fee_asset_item: u32,
	weight_limit: WeightLimit,
	beneficiary: MultiLocation,
	destination: MultiLocation,
) -> u128 {
	// Approximation of the actual message sent by the extrinsic.
	// The assets are not reanchored and the topic is a dummy one.
	// However, it should have the same encoded size, which is what matters for delivery fees.
	let message = Xcm(vec![
		ReceiveTeleportedAsset(assets.clone()), // Same encoded size as `ReserveAssetDeposited`
		ClearOrigin,
		BuyExecution { fees: assets.get(fee_asset_item as usize).unwrap().clone(), weight_limit },
		DepositAsset { assets: Wild(AllCounted(assets.len() as u32)), beneficiary },
		SetTopic([0u8; 32]), // Dummy topic
	]);
	let Parachain(para_id) = destination.interior().last().unwrap() else { unreachable!("Location is parachain") };
	let delivery_fees = P::price_for_parachain_delivery((*para_id).into(), &message);
	let Fungible(delivery_fees_amount) = delivery_fees.inner()[0].fun else { unreachable!("Asset is fungible") };
	delivery_fees_amount
}
