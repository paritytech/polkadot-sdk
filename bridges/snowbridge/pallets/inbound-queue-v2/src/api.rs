// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implements the dry-run API.

use crate::{Config, Error};
use snowbridge_inbound_queue_primitives::v2::{ConvertMessage, Message};
use sp_runtime::DispatchError;
use xcm::latest::Xcm;

pub fn convert_message<T>(message: Message) -> Result<Xcm<()>, DispatchError>
where
	T: Config,
{
	T::MessageConverter::convert(message).map_err(|e| Error::<T>::from(e).into())
}
