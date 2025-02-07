// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implements the dry-run API.

use crate::{weights::WeightInfo, Config, Error, Location};
use frame_support::weights::WeightToFee;
use snowbridge_inbound_queue_primitives::v2::{ConvertMessage, Message};
use sp_core::Get;
use sp_runtime::DispatchError;
use xcm::{
	latest::Xcm,
	opaque::latest::{validate_send, Asset, Fungibility, Junction::Parachain},
};

pub fn dry_run<T>(message: Message) -> Result<(Xcm<()>, T::Balance), DispatchError>
where
	T: Config,
{
	// Convert the inbound message into an XCM message.
	let xcm = T::MessageConverter::convert(message).map_err(|e| Error::<T>::from(e))?;

	// Compute the base fee for submitting the extrinsic. This covers the cost of the "submit" call
	// on our chain.
	let submit_weight_fee = T::WeightToFee::weight_to_fee(&T::WeightInfo::submit());
	let mut total_fee: u128 = submit_weight_fee.try_into().map_err(|_| Error::<T>::InvalidFee)?;

	// Include the delivery fee from the Asset Hub side by validating the xcm message send.
	//  This returns a list (`Assets`) of fees required.
	let destination = Location::new(1, [Parachain(T::AssetHubParaId::get())]);
	let (_, delivery_assets) = validate_send::<T::XcmSender>(destination, xcm.clone())
		.map_err(|_| Error::<T>::InvalidFee)?;

	// Sum up any fungible assets returned in `delivery_assets`.
	for asset in delivery_assets.into_inner() {
		if let Asset { fun: Fungibility::Fungible(amount), .. } = asset {
			total_fee = total_fee.saturating_add(amount);
		}
	}

	// Return the XCM message and the total fee (Ether, Dot) (converted into T::Balance).
	Ok((xcm, total_fee.into()))
}
