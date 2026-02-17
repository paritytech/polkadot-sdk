// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Message processor for inbound queue v2

use super::*;
use frame_support::traits::Get;
use sp_runtime::traits::TryConvert;
use sp_std::marker::PhantomData;
use xcm::prelude::*;

/// A message processor that converts messages to XCM and forwards them to AssetHub
/// Generic parameters: T = pallet Config, Sender = XCM sender, Executor = fee handler,
/// Converter = message converter, AccountToLocation = account-to-location converter
pub struct XcmMessageProcessor<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>(
	pub PhantomData<(T, Sender, Executor, Converter, AccountToLocation, TargetLocation)>,
);

impl<AccountId, T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
	MessageProcessor<AccountId>
	for XcmMessageProcessor<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
where
	T: frame_system::Config<AccountId = AccountId>,
	Sender: SendXcm,
	Executor: ExecuteXcm<T::RuntimeCall>,
	Converter: ConvertMessage,
	AccountToLocation: for<'a> TryConvert<&'a AccountId, Location>,
	TargetLocation: Get<Location>,
{
	fn can_process_message(_relayer: &AccountId, _message: &Message) -> bool {
		true
	}

	fn process_message(
		relayer: AccountId,
		message: Message,
	) -> Result<[u8; 32], MessageProcessorError> {
		// Process the message and return its ID
		let id = Self::process_xcm(relayer, message)?;
		Ok(id)
	}
}

impl<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
	XcmMessageProcessor<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
where
	T: frame_system::Config,
	Sender: SendXcm,
	Executor: ExecuteXcm<T::RuntimeCall>,
	Converter: ConvertMessage,
	AccountToLocation: for<'a> TryConvert<&'a T::AccountId, Location>,
	TargetLocation: Get<Location>,
{
	/// Process a message and return the message ID
	pub fn process_xcm(
		who: T::AccountId,
		message: Message,
	) -> Result<XcmHash, MessageProcessorError> {
		// Convert the message to XCM
		let xcm = Converter::convert(message).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, "XCM conversion failed with error");
			MessageProcessorError::ConvertMessage(error)
		})?;

		// Forward XCM to a target location
		let dest = TargetLocation::get();
		let message_id = Self::send_xcm(dest.clone(), &who, xcm.clone()).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, ?dest, ?xcm, "XCM send failed with error");
			MessageProcessorError::SendMessage(error)
		})?;

		// Return the message_id
		Ok(message_id)
	}
}

impl<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
	XcmMessageProcessor<T, Sender, Executor, Converter, AccountToLocation, TargetLocation>
where
	T: frame_system::Config,
	Sender: SendXcm,
	Executor: ExecuteXcm<T::RuntimeCall>,
	Converter: ConvertMessage,
	AccountToLocation: for<'a> TryConvert<&'a T::AccountId, Location>,
	TargetLocation: Get<Location>,
{
	fn send_xcm(
		dest: Location,
		fee_payer: &T::AccountId,
		xcm: Xcm<()>,
	) -> Result<XcmHash, SendError> {
		let fee_payer = AccountToLocation::try_convert(fee_payer).map_err(|err| {
			tracing::error!(
				target: LOG_TARGET,
				?err,
				"Failed to convert account to XCM location",
			);
			SendError::NotApplicable
		})?;
		let (ticket, fee) = validate_send::<Sender>(dest, xcm)?;
		Executor::charge_fees(fee_payer, fee).map_err(|error| {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				"Charging fees failed with error",
			);
			SendError::Fees
		})?;
		Sender::deliver(ticket)
	}
}
