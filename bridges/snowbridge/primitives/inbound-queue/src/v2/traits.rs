// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2025 Snowfork <hello@snowfork.com>
// SPDX-FileCopyrightText: 2021-2025 Parity Technologies (UK) Ltd.
use super::Message;
use sp_runtime::{DispatchError, Weight};
use xcm::latest::{SendError, Xcm};
use Debug;

/// Converts an inbound message from Ethereum to an XCM message that can be
/// executed on a parachain.
pub trait ConvertMessage {
	fn convert(message: Message) -> Result<Xcm<()>, ConvertMessageError>;
}

/// Reason why a message conversion failed.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ConvertMessageError {
	/// Invalid foreign ERC-20 token ID
	InvalidAsset,
	/// Cannot reachor a foreign ERC-20 asset location.
	CannotReanchor,
	/// Invalid network specified (not from Ethereum)
	InvalidNetwork,
}

/// Reason why a message processor failed.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageProcessorError {
	/// Message processing failed.
	ProcessMessage(DispatchError),
	/// Message conversion failed.
	ConvertMessage(ConvertMessageError),
	/// Message sending failed.
	SendMessage(SendError),
}

/// Trait to define the logic for checking and processing inbound messages.
pub trait MessageProcessor<AccountId> {
	/// Lightweight function to check if this processor can handle the message
	fn can_process_message(relayer: &AccountId, message: &Message) -> bool;
	/// Process the message and return the message ID
	fn process_message(
		relayer: AccountId,
		message: Message,
	) -> Result<([u8; 32], Option<Weight>), MessageProcessorError>;

	/// Returns the worst case message processor weight
	fn worst_case_message_processor_weight() -> Weight {
		Weight::default()
	}
}

#[impl_trait_for_tuples::impl_for_tuples(10)]
impl<AccountId> MessageProcessor<AccountId> for Tuple {
	fn can_process_message(relayer: &AccountId, message: &Message) -> bool {
		for_tuples!( #(
 			match Tuple::can_process_message(&relayer, &message) {
				true => {
					return true;
				},
				_ => {}
			}
		)* );

		false
	}

	fn process_message(
		relayer: AccountId,
		message: Message,
	) -> Result<([u8; 32], Option<Weight>), MessageProcessorError> {
		for_tuples!( #(
 			match Tuple::can_process_message(&relayer, &message) {
				true => {
					return Tuple::process_message(relayer, message)
				},
				_ => {}
			}
		)* );

		Err(MessageProcessorError::ProcessMessage(DispatchError::Other(
			"No handler found for message!",
		)))
	}

	fn worst_case_message_processor_weight() -> Weight {
		let mut max_weight = Weight::zero();

		for_tuples!( #(
			max_weight = max_weight.max(
				Tuple::worst_case_message_processor_weight()
			);
		)* );

		max_weight
	}
}
