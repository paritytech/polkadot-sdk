// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Errors for the XCM pallet.

use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::PalletError;
use scale_info::TypeInfo;
use xcm::latest::Error as XcmError;

#[derive(
	Copy, Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, PalletError,
)]
pub enum ExecutionError {
	// Errors that happen due to instructions being executed. These alone are defined in the
	// XCM specification.
	/// An arithmetic overflow happened.
	Overflow,
	/// The instruction is intentionally unsupported.
	Unimplemented,
	/// Origin Register does not contain a value value for a reserve transfer notification.
	UntrustedReserveLocation,
	/// Origin Register does not contain a value value for a teleport notification.
	UntrustedTeleportLocation,
	/// `MultiLocation` value too large to descend further.
	LocationFull,
	/// `MultiLocation` value ascend more parents than known ancestors of local location.
	LocationNotInvertible,
	/// The Origin Register does not contain a valid value for instruction.
	BadOrigin,
	/// The location parameter is not a valid value for the instruction.
	InvalidLocation,
	/// The given asset is not handled.
	AssetNotFound,
	/// An asset transaction (like withdraw or deposit) failed (typically due to type conversions).
	FailedToTransactAsset,
	/// An asset cannot be withdrawn, potentially due to lack of ownership, availability or rights.
	NotWithdrawable,
	/// An asset cannot be deposited under the ownership of a particular location.
	LocationCannotHold,
	/// Attempt to send a message greater than the maximum supported by the transport protocol.
	ExceedsMaxMessageSize,
	/// The given message cannot be translated into a format supported by the destination.
	DestinationUnsupported,
	/// Destination is routable, but there is some issue with the transport mechanism.
	Transport,
	/// Destination is known to be unroutable.
	Unroutable,
	/// Used by `ClaimAsset` when the given claim could not be recognized/found.
	UnknownClaim,
	/// Used by `Transact` when the functor cannot be decoded.
	FailedToDecode,
	/// Used by `Transact` to indicate that the given weight limit could be breached by the
	/// functor.
	MaxWeightInvalid,
	/// Used by `BuyExecution` when the Holding Register does not contain payable fees.
	NotHoldingFees,
	/// Used by `BuyExecution` when the fees declared to purchase weight are insufficient.
	TooExpensive,
	/// Used by the `Trap` instruction to force an error intentionally. Its code is included.
	Trap,
	/// Used by `ExpectAsset`, `ExpectError` and `ExpectOrigin` when the expectation was not true.
	ExpectationFalse,
	/// The provided pallet index was not found.
	PalletNotFound,
	/// The given pallet's name is different to that expected.
	NameMismatch,
	/// The given pallet's version has an incompatible version to that expected.
	VersionIncompatible,
	/// The given operation would lead to an overflow of the Holding Register.
	HoldingWouldOverflow,
	/// The message was unable to be exported.
	ExportError,
	/// `MultiLocation` value failed to be reanchored.
	ReanchorFailed,
	/// No deal is possible under the given constraints.
	NoDeal,
	/// Fees were required which the origin could not pay.
	FeesNotMet,
	/// Some other error with locking.
	LockError,
	/// The state was not in a condition where the operation was valid to make.
	NoPermission,
	/// The universal location of the local consensus is improper.
	Unanchored,
	/// An asset cannot be deposited, probably because (too much of) it already exists.
	NotDepositable,
	/// Too many assets matched the given asset filter.
	TooManyAssets,
	// Errors that happen prior to instructions being executed. These fall outside of the XCM
	// spec.
	/// XCM version not able to be handled.
	UnhandledXcmVersion,
	/// Execution of the XCM would potentially result in a greater weight used than weight limit.
	WeightLimitReached,
	/// The XCM did not pass the barrier condition for execution.
	///
	/// The barrier condition differs on different chains and in different circumstances, but
	/// generally it means that the conditions surrounding the message were not such that the chain
	/// considers the message worth spending time executing. Since most chains lift the barrier to
	/// execution on appropriate payment, presentation of an NFT voucher, or based on the message
	/// origin, it means that none of those were the case.
	Barrier,
	/// The weight of an XCM message is not computable ahead of execution.
	WeightNotComputable,
	/// Recursion stack limit reached
	// TODO(https://github.com/paritytech/polkadot-sdk/issues/6199): This should have a fixed index since
	// we use it in `FrameTransactionalProcessor` // which is used in instructions.
	// Or we should create a different error for that.
	ExceedsStackLimit,
}

impl From<XcmError> for ExecutionError {
	fn from(error: XcmError) -> Self {
		match error {
			XcmError::Overflow => Self::Overflow,
			XcmError::Unimplemented => Self::Unimplemented,
			XcmError::UntrustedReserveLocation => Self::UntrustedReserveLocation,
			XcmError::UntrustedTeleportLocation => Self::UntrustedTeleportLocation,
			XcmError::LocationFull => Self::LocationFull,
			XcmError::LocationNotInvertible => Self::LocationNotInvertible,
			XcmError::BadOrigin => Self::BadOrigin,
			XcmError::InvalidLocation => Self::InvalidLocation,
			XcmError::AssetNotFound => Self::AssetNotFound,
			XcmError::FailedToTransactAsset(_) => Self::FailedToTransactAsset,
			XcmError::NotWithdrawable => Self::NotWithdrawable,
			XcmError::LocationCannotHold => Self::LocationCannotHold,
			XcmError::ExceedsMaxMessageSize => Self::ExceedsMaxMessageSize,
			XcmError::DestinationUnsupported => Self::DestinationUnsupported,
			XcmError::Transport(_) => Self::Transport,
			XcmError::Unroutable => Self::Unroutable,
			XcmError::UnknownClaim => Self::UnknownClaim,
			XcmError::FailedToDecode => Self::FailedToDecode,
			XcmError::MaxWeightInvalid => Self::MaxWeightInvalid,
			XcmError::NotHoldingFees => Self::NotHoldingFees,
			XcmError::TooExpensive => Self::TooExpensive,
			XcmError::Trap(_) => Self::Trap,
			XcmError::ExpectationFalse => Self::ExpectationFalse,
			XcmError::PalletNotFound => Self::PalletNotFound,
			XcmError::NameMismatch => Self::NameMismatch,
			XcmError::VersionIncompatible => Self::VersionIncompatible,
			XcmError::HoldingWouldOverflow => Self::HoldingWouldOverflow,
			XcmError::ExportError => Self::ExportError,
			XcmError::ReanchorFailed => Self::ReanchorFailed,
			XcmError::NoDeal => Self::NoDeal,
			XcmError::FeesNotMet => Self::FeesNotMet,
			XcmError::LockError => Self::LockError,
			XcmError::NoPermission => Self::NoPermission,
			XcmError::Unanchored => Self::Unanchored,
			XcmError::NotDepositable => Self::NotDepositable,
			XcmError::TooManyAssets => Self::TooManyAssets,
			XcmError::UnhandledXcmVersion => Self::UnhandledXcmVersion,
			XcmError::WeightLimitReached(_) => Self::WeightLimitReached,
			XcmError::Barrier => Self::Barrier,
			XcmError::WeightNotComputable => Self::WeightNotComputable,
			XcmError::ExceedsStackLimit => Self::ExceedsStackLimit,
		}
	}
}
