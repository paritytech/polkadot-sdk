// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implementation for [`frame_support::traits::ProcessMessage`]
use super::*;
use crate::weights::WeightInfo;
use frame_support::{
	traits::{ProcessMessage, ProcessMessageError},
	weights::WeightMeter,
};

impl<T: Config> ProcessMessage for Pallet<T> {
	type Origin = T::AggregateMessageOrigin;
	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		_: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let weight = T::WeightInfo::do_process_message();
		if meter.try_consume(weight).is_err() {
			Self::deposit_event(Event::MessagePostponed {
				payload: message.to_vec(),
				reason: ProcessMessageError::Overweight(weight),
			});
			return Err(ProcessMessageError::Overweight(weight))
		}
		Self::do_process_message(origin, message)
	}
}
