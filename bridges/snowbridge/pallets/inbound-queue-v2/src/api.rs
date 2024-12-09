// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implements the dry-run API.

use crate::{weights::WeightInfo, Config, Error, Junction::AccountId32, Location};
use frame_support::weights::WeightToFee;
use snowbridge_router_primitives::inbound::v2::{ConvertMessage, Message};
use sp_core::H256;
use sp_runtime::DispatchError;
use xcm::latest::Xcm;

pub fn dry_run<T>(message: Message) -> Result<(Xcm<()>, T::Balance), DispatchError>
where
	T: Config,
{
	// Convert message to XCM
	let dummy_origin = Location::new(0, AccountId32 { id: H256::zero().into(), network: None });
	let xcm = T::MessageConverter::convert(message, dummy_origin)
		.map_err(|e| Error::<T>::ConvertMessage(e))?;

	// Calculate fee. Consists of the cost of the "submit" extrinsic as well as the XCM execution
	// prologue fee (static XCM part of the message that is execution on AH).
	let weight_fee = T::WeightToFee::weight_to_fee(&T::WeightInfo::submit());
	let fee: u128 = weight_fee.try_into().map_err(|_| Error::<T>::InvalidFee)?;

	Ok((xcm, fee.into()))
}
