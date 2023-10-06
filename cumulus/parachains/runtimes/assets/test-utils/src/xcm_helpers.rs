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

//! Helpers for calculating XCM delivery fees.

use xcm::latest::prelude::*;

/// Returns the delivery fees amount for pallet xcm's `teleport_assets` and
/// `reserve_transfer_assets` extrinsics.
/// Because it returns only a `u128`, it assumes delivery fees are only paid
/// in one asset and that asset is known.
pub fn transfer_assets_delivery_fees<S: SendXcm>(
	assets: MultiAssets,
	fee_asset_item: u32,
	weight_limit: WeightLimit,
	beneficiary: MultiLocation,
	destination: MultiLocation,
) -> u128 {
	let message = teleport_assets_dummy_message(assets, fee_asset_item, weight_limit, beneficiary);
	let Ok((_, delivery_fees)) = validate_send::<S>(destination, message) else { unreachable!("message can be sent; qed") };
	if let Some(delivery_fee) = delivery_fees.inner().first() {
		let Fungible(delivery_fee_amount) = delivery_fee.fun else { unreachable!("asset is fungible; qed"); };
		delivery_fee_amount
	} else {
		0
	}
}

/// Approximates the actual message sent by the teleport extrinsic.
/// The assets are not reanchored and the topic is a dummy one.
/// However, it should have the same encoded size, which is what matters for delivery fees.
/// Also has same encoded size as the one created by the reserve transfer assets extrinsic.
fn teleport_assets_dummy_message(
	assets: MultiAssets,
	fee_asset_item: u32,
	weight_limit: WeightLimit,
	beneficiary: MultiLocation,
) -> Xcm<()> {
	Xcm(vec![
		ReceiveTeleportedAsset(assets.clone()), // Same encoded size as `ReserveAssetDeposited`
		ClearOrigin,
		BuyExecution { fees: assets.get(fee_asset_item as usize).unwrap().clone(), weight_limit },
		DepositAsset { assets: Wild(AllCounted(assets.len() as u32)), beneficiary },
		SetTopic([0u8; 32]), // Dummy topic
	])
}
