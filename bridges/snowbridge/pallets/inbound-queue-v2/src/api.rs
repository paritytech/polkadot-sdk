// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implements the dry-run API.

use crate::{Config, Error, Location};
use snowbridge_inbound_queue_primitives::v2::{ConvertMessage, Message};
use sp_runtime::DispatchError;
use xcm::latest::Xcm;
use xcm::opaque::latest::Junction::Parachain;
use xcm::opaque::latest::validate_send;
use sp_core::Get;
use frame_support::weights::WeightToFee;
use xcm::opaque::latest::Fungibility;
use xcm::opaque::latest::Asset;
use crate::weights::WeightInfo;


pub fn dry_run<T>(message: Message) -> Result<(Xcm<()>, (T::Balance, T::Balance)), DispatchError>
	where
		T: Config,
{
	// Convert the inbound message into an XCM message. Passing `[0; 32]` here as a placeholder message_id
	let xcm = T::MessageConverter::convert(message, [0; 32])
		.map_err(|e| Error::<T>::from(e))?;

	// Compute the base fee for submitting the extrinsic. This covers the cost of the "submit" call
	// on our chain.
	let submit_weight_fee = T::WeightToFee::weight_to_fee(&T::WeightInfo::submit());
	let eth_fee: u128 = submit_weight_fee
		.try_into()
		.map_err(|_| Error::<T>::InvalidFee)?;

	// Include the delivery fee from the Asset Hub side by validating the xcm message send.
	//  This returns a list (`Assets`) of fees required.
	let destination = Location::new(1, [Parachain(T::AssetHubParaId::get())]);
	let (_, delivery_assets) = validate_send::<T::XcmSender>(destination, xcm.clone())
		.map_err(|_| Error::<T>::InvalidFee)?;

	let mut dot_fee = 0;
	// Sum up any fungible assets returned in `delivery_assets`.
	for asset in delivery_assets.into_inner() {
		if let Asset {
			fun: Fungibility::Fungible(amount),
			..
		} = asset
		{
			dot_fee = amount;
		}
	}

	// Return the XCM message and the total fee (Ether, Dot) (converted into T::Balance).
	Ok((xcm, (eth_fee.into(), dot_fee.into())))
}
