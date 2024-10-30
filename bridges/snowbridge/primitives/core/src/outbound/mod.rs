// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Outbound
//!
//! Common traits and types
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::RuntimeDebug;

pub mod v1;
pub mod v2;

/// The operating mode of Channels and Gateway contract on Ethereum.
#[derive(Copy, Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum OperatingMode {
	/// Normal operations. Allow sending and receiving messages.
	Normal,
	/// Reject outbound messages. This allows receiving governance messages but does now allow
	/// enqueuing of new messages from the Ethereum side. This can be used to close off an
	/// deprecated channel or pause the bridge for upgrade operations.
	RejectingOutboundMessages,
}

/// A trait for getting the local costs associated with sending a message.
pub trait SendMessageFeeProvider {
	type Balance: BaseArithmetic + Unsigned + Copy;

	/// The local component of the message processing fees in native currency
	fn local_fee() -> Self::Balance;
}
