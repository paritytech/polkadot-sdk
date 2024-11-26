// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implements the dry-run API.

use crate::{Config, Error, Junction::AccountId32, Location};
use snowbridge_core::inbound::Proof;
use snowbridge_router_primitives::inbound::v2::{ConvertMessage, Message};
use sp_core::H256;
use xcm::latest::Xcm;

pub fn dry_run<T>(message: Message, _proof: Proof) -> Result<Xcm<()>, Error<T>>
where
	T: Config,
{
	let dummy_origin = Location::new(0, AccountId32 { id: H256::zero().into(), network: None });
	let xcm = T::MessageConverter::convert(message, dummy_origin)
		.map_err(|e| Error::<T>::ConvertMessage(e))?;
	Ok(xcm)
}
