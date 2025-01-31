// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]
//! # Outbound
//!
//! Common traits and types
pub mod v1;
pub mod v2;

use codec::{Decode, Encode};
use frame_support::PalletError;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::{RuntimeDebug, H160};
use sp_std::vec::Vec;

pub use snowbridge_verification_primitives::*;

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

/// Reasons why sending to Ethereum could not be initiated
#[derive(Copy, Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, PalletError, TypeInfo)]
pub enum SendError {
	/// Message is too large to be safely executed on Ethereum
	MessageTooLarge,
	/// The bridge has been halted for maintenance
	Halted,
	/// Invalid Channel
	InvalidChannel,
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum DryRunError {
	ConvertLocationFailed,
	ConvertXcmFailed,
}

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct TransactInfo {
	pub target: H160,
	pub data: Vec<u8>,
	pub gas_limit: u64,
	pub value: u128,
}
