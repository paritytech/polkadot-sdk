// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Transaction pool errors.

use sr_primitives::transaction_validity::TransactionPriority as Priority;

/// Transaction pool result.
pub type Result<T> = std::result::Result<T, Error>;

/// Transaction pool error type.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
	/// Transaction is not verifiable yet, but might be in the future.
	#[display(fmt="Unkown Transaction Validity. Error code: {}", _0)]
	UnknownTransactionValidity(i8),
	/// Transaction is invalid.
	#[display(fmt="Invalid Transaction. Error Code: {}", _0)]
	InvalidTransaction(i8),
	/// The transaction is temporarily banned.
	#[display(fmt="Temporarily Banned")]
	TemporarilyBanned,
	/// The transaction is already in the pool.
	#[display(fmt="[{:?}] Already imported", _0)]
	AlreadyImported(Box<dyn std::any::Any + Send>),
	/// The transaction cannot be imported cause it's a replacement and has too low priority.
	#[display(fmt="Too low priority ({} > {})", old, new)]
	TooLowPriority {
		/// Transaction already in the pool.
		old: Priority,
		/// Transaction entering the pool.
		new: Priority
	},
	/// Deps cycle etected and we couldn't import transaction.
	#[display(fmt="Cycle Detected")]
	CycleDetected,
	/// Transaction was dropped immediately after it got inserted.
	#[display(fmt="Transaction couldn't enter the pool because of the limit.")]
	ImmediatelyDropped,
	/// Invalid block id.
	InvalidBlockId(String),
}

impl std::error::Error for Error {}

/// Transaction pool error conversion.
pub trait IntoPoolError: ::std::error::Error + Send + Sized {
	/// Try to extract original `Error`
	///
	/// This implementation is optional and used only to
	/// provide more descriptive error messages for end users
	/// of RPC API.
	fn into_pool_error(self) -> ::std::result::Result<Error, Self> { Err(self) }
}

impl IntoPoolError for Error {
	fn into_pool_error(self) -> ::std::result::Result<Error, Self> { Ok(self) }
}
