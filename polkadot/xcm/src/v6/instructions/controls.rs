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

//! Control flow related instructions.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use bounded_collections::BoundedVec;
use educe::Educe;

use crate::v6::{Xcm, Hint, HintNumVariants};

/// Set the Error Handler Register. This is code that should be called in the case of an error
/// happening.
///
/// An error occurring within execution of this code will _NOT_ result in the error register
/// being set, nor will an error handler be called due to it. The error handler and appendix
/// may each still be set.
///
/// The apparent weight of this instruction is inclusive of the inner `Xcm`; the executing
/// weight however includes only the difference between the previous handler and the new
/// handler, which can reasonably be negative, which would result in a surplus.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone(bound = false), Eq, PartialEq(bound = false), Debug(bound = false))]
#[scale_info(skip_type_params(Call))]
pub struct SetErrorHandler<Call: 'static>(pub Xcm<Call>);

impl<Call> SetErrorHandler<Call> {
	pub fn into<C>(self) -> SetErrorHandler<C> {
		SetErrorHandler::from(self)
	}

	pub fn from<C>(xcm: SetErrorHandler<C>) -> Self {
		Self(xcm.0.into())
	}
}

/// Set the Appendix Register. This is code that should be called after code execution
/// (including the error handler if any) is finished. This will be called regardless of whether
/// an error occurred.
///
/// Any error occurring due to execution of this code will result in the error register being
/// set, and the error handler (if set) firing.
///
/// The apparent weight of this instruction is inclusive of the inner `Xcm`; the executing
/// weight however includes only the difference between the previous appendix and the new
/// appendix, which can reasonably be negative, which would result in a surplus.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone(bound = false), Eq, PartialEq(bound = false), Debug(bound = false))]
#[scale_info(skip_type_params(Call))]
pub struct SetAppendix<Call: 'static>(pub Xcm<Call>);

impl<Call> SetAppendix<Call> {
	pub fn into<C>(self) -> SetAppendix<C> {
		SetAppendix::from(self)
	}

	pub fn from<C>(xcm: SetAppendix<C>) -> Self {
		Self(xcm.0.into())
	}
}

/// Clear the Error Register.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ClearError;

/// Always throws an error of type `Trap`.
///
/// Kind: *Command*
///
/// Errors:
/// - `Trap`: All circumstances, whose inner value is the same as this item's inner value.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct Trap(#[codec(compact)] pub u64);

/// Set the Transact Status Register to its default, cleared, value.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Infallible*.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ClearTransactStatus;

/// Sets the Fees Mode Register.
///
/// - `jit_withdraw`: The fees mode item; if set to `true` then fees for any instructions are
///   withdrawn as needed using the same mechanism as `WithdrawAssets`.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct SetFeesMode {
	pub jit_withdraw: bool,
}

/// Set the Topic Register.
///
/// The 32-byte array identifier in the parameter is not guaranteed to be
/// unique; if such a property is desired, it is up to the code author to
/// enforce uniqueness.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct SetTopic(pub [u8; 32]);

/// Clear the Topic Register.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ClearTopic;

/// Set hints for XCM execution.
///
/// These hints change the behaviour of the XCM program they are present in.
///
/// Parameters:
///
/// - `hints`: A bounded vector of `ExecutionHint`, specifying the different hints that will
/// be activated.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct SetHints {
	pub hints: BoundedVec<Hint, HintNumVariants>,
}
