// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The traits for dealing with a single fungible token class and any associated types.
//!
//! ### User-implememted traits
//! - `Inspect`: Regular balance inspector functions.
//! - `Unbalanced`: Low-level balance mutating functions. Does not guarantee proper book-keeping and
//!   so should not be called into directly from application code. Other traits depend on this and
//!   provide default implementations based on it.
//! - `UnbalancedHold`: Low-level balance mutating functions for balances placed on hold. Does not
//!   guarantee proper book-keeping and so should not be called into directly from application code.
//!   Other traits depend on this and provide default implementations based on it.
//! - `Mutate`: Regular balance mutator functions. Pre-implemented using `Unbalanced`, though the
//!   `done_*` functions should likely be reimplemented in case you want to do something following
//!   the operation such as emit events.
//! - `InspectHold`: Inspector functions for balances on hold.
//! - `MutateHold`: Mutator functions for balances on hold. Mostly pre-implemented using
//!   `UnbalancedHold`.
//! - `InspectFreeze`: Inspector functions for frozen balance.
//! - `MutateFreeze`: Mutator functions for frozen balance.
//! - `Balanced`: One-sided mutator functions for regular balances, which return imbalance objects
//!   which guarantee eventual book-keeping. May be useful for some sophisticated operations where
//!   funds must be removed from an account before it is known precisely what should be done with
//!   them.

pub mod conformance_tests;
pub mod freeze;
pub mod hold;
pub(crate) mod imbalance;
mod item_of;
mod regular;
mod union_of;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support_procedural::{CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

use super::{
	Fortitude::{Force, Polite},
	Precision::BestEffort,
};
pub use freeze::{Inspect as InspectFreeze, Mutate as MutateFreeze};
pub use hold::{
	Balanced as BalancedHold, Inspect as InspectHold, Mutate as MutateHold,
	Unbalanced as UnbalancedHold,
};
pub use imbalance::{Credit, Debt, HandleImbalanceDrop, Imbalance};
pub use item_of::ItemOf;
pub use regular::{
	Balanced, DecreaseIssuance, Dust, IncreaseIssuance, Inspect, Mutate, Unbalanced,
};
use sp_arithmetic::traits::Zero;
use sp_core::Get;
use sp_runtime::{traits::Convert, DispatchError};
pub use union_of::{NativeFromLeft, NativeOrWithId, UnionOf};

use crate::{
	ensure,
	traits::{Consideration, Footprint},
};

/// Consideration method using a `fungible` balance frozen as the cost exacted for the footprint.
///
/// The aggregate amount frozen under `R::get()` for any account which has multiple tickets,
/// is the *cumulative* amounts of each ticket's footprint (each individually determined by `D`).
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(A, F, R, D))]
#[codec(mel_bound())]
pub struct FreezeConsideration<A, F, R, D>(F::Balance, PhantomData<fn() -> (A, R, D)>)
where
	F: MutateFreeze<A>;
impl<
		A: 'static,
		F: 'static + MutateFreeze<A>,
		R: 'static + Get<F::Id>,
		D: 'static + Convert<Footprint, F::Balance>,
	> Consideration<A> for FreezeConsideration<A, F, R, D>
{
	fn new(who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		let new = D::convert(footprint);
		F::increase_frozen(&R::get(), who, new)?;
		Ok(Self(new, PhantomData))
	}
	fn update(self, who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		let new = D::convert(footprint);
		if self.0 > new {
			F::decrease_frozen(&R::get(), who, self.0 - new)?;
		} else if new > self.0 {
			F::increase_frozen(&R::get(), who, new - self.0)?;
		}
		Ok(Self(new, PhantomData))
	}
	fn drop(self, who: &A) -> Result<(), DispatchError> {
		F::decrease_frozen(&R::get(), who, self.0).map(|_| ())
	}
}

/// Consideration method using a `fungible` balance frozen as the cost exacted for the footprint.
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(A, F, R, D))]
#[codec(mel_bound())]
pub struct HoldConsideration<A, F, R, D>(F::Balance, PhantomData<fn() -> (A, R, D)>)
where
	F: MutateHold<A>;
impl<
		A: 'static,
		F: 'static + MutateHold<A>,
		R: 'static + Get<F::Reason>,
		D: 'static + Convert<Footprint, F::Balance>,
	> Consideration<A> for HoldConsideration<A, F, R, D>
{
	fn new(who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		let new = D::convert(footprint);
		F::hold(&R::get(), who, new)?;
		Ok(Self(new, PhantomData))
	}
	fn update(self, who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		let new = D::convert(footprint);
		if self.0 > new {
			F::release(&R::get(), who, self.0 - new, BestEffort)?;
		} else if new > self.0 {
			F::hold(&R::get(), who, new - self.0)?;
		}
		Ok(Self(new, PhantomData))
	}
	fn drop(self, who: &A) -> Result<(), DispatchError> {
		F::release(&R::get(), who, self.0, BestEffort).map(|_| ())
	}
	fn burn(self, who: &A) {
		let _ = F::burn_held(&R::get(), who, self.0, BestEffort, Force);
	}
}

/// Basic consideration method using a `fungible` balance frozen as the cost exacted for the
/// footprint.
///
/// NOTE: This is an optimized implementation, which can only be used for systems where each
/// account has only a single active ticket associated with it since individual tickets do not
/// track the specific balance which is frozen. If you are uncertain then use `FreezeConsideration`
/// instead, since this works in all circumstances.
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(A, Fx, Rx, D))]
#[codec(mel_bound())]
pub struct LoneFreezeConsideration<A, Fx, Rx, D>(PhantomData<fn() -> (A, Fx, Rx, D)>);
impl<
		A: 'static,
		Fx: 'static + MutateFreeze<A>,
		Rx: 'static + Get<Fx::Id>,
		D: 'static + Convert<Footprint, Fx::Balance>,
	> Consideration<A> for LoneFreezeConsideration<A, Fx, Rx, D>
{
	fn new(who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		ensure!(Fx::balance_frozen(&Rx::get(), who).is_zero(), DispatchError::Unavailable);
		Fx::set_frozen(&Rx::get(), who, D::convert(footprint), Polite).map(|_| Self(PhantomData))
	}
	fn update(self, who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		Fx::set_frozen(&Rx::get(), who, D::convert(footprint), Polite).map(|_| Self(PhantomData))
	}
	fn drop(self, who: &A) -> Result<(), DispatchError> {
		Fx::thaw(&Rx::get(), who).map(|_| ())
	}
}

/// Basic consideration method using a `fungible` balance placed on hold as the cost exacted for the
/// footprint.
///
/// NOTE: This is an optimized implementation, which can only be used for systems where each
/// account has only a single active ticket associated with it since individual tickets do not
/// track the specific balance which is frozen. If you are uncertain then use `FreezeConsideration`
/// instead, since this works in all circumstances.
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(A, Fx, Rx, D))]
#[codec(mel_bound())]
pub struct LoneHoldConsideration<A, Fx, Rx, D>(PhantomData<fn() -> (A, Fx, Rx, D)>);
impl<
		A: 'static,
		F: 'static + MutateHold<A>,
		R: 'static + Get<F::Reason>,
		D: 'static + Convert<Footprint, F::Balance>,
	> Consideration<A> for LoneHoldConsideration<A, F, R, D>
{
	fn new(who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		ensure!(F::balance_on_hold(&R::get(), who).is_zero(), DispatchError::Unavailable);
		F::set_on_hold(&R::get(), who, D::convert(footprint)).map(|_| Self(PhantomData))
	}
	fn update(self, who: &A, footprint: Footprint) -> Result<Self, DispatchError> {
		F::set_on_hold(&R::get(), who, D::convert(footprint)).map(|_| Self(PhantomData))
	}
	fn drop(self, who: &A) -> Result<(), DispatchError> {
		F::release_all(&R::get(), who, BestEffort).map(|_| ())
	}
	fn burn(self, who: &A) {
		let _ = F::burn_all_held(&R::get(), who, BestEffort, Force);
	}
}
