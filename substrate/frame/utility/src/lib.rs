// This file is part of Substrate.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
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

//! # Utility Module
//! A stateless module with helpers for dispatch management which does no re-authentication.
//!
//! - [`utility::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//!
//! ## Overview
//!
//! This module contains two basic pieces of functionality:
//! - Batch dispatch: A stateless operation, allowing any origin to execute multiple calls in a
//!   single dispatch. This can be useful to amalgamate proposals, combining `set_code` with
//!   corresponding `set_storage`s, for efficient multiple payouts with just a single signature
//!   verify, or in combination with one of the other two dispatch functionality.
//! - Pseudonymal dispatch: A stateless operation, allowing a signed origin to execute a call from
//!   an alternative signed origin. Each account has 2 * 2**16 possible "pseudonyms" (alternative
//!   account IDs) and these can be stacked. This can be useful as a key management tool, where you
//!   need multiple distinct accounts (e.g. as controllers for many staking accounts), but where
//!   it's perfectly fine to have each of them controlled by the same underlying keypair.
//!   Derivative accounts are, for the purposes of proxy filtering considered exactly the same as
//!   the oigin and are thus hampered with the origin's filters.
//!
//! Since proxy filters are respected in all dispatches of this module, it should never need to be
//! filtered by any proxy.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! #### For batch dispatch
//! * `batch` - Dispatch multiple calls from the sender's origin.
//!
//! #### For pseudonymal dispatch
//! * `as_derivative` - Dispatch a call from a derivative signed origin.
//!
//! [`Call`]: ./enum.Call.html
//! [`Trait`]: ./trait.Trait.html

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::prelude::*;
use codec::{Encode, Decode};
use sp_core::TypeId;
use sp_io::hashing::blake2_256;
use frame_support::{decl_module, decl_event, decl_storage, Parameter};
use frame_support::{
	traits::{OriginTrait, UnfilteredDispatchable},
	weights::{Weight, GetDispatchInfo, DispatchClass}, dispatch::PostDispatchInfo,
};
use frame_system::{self as system, ensure_signed, ensure_root};
use sp_runtime::{DispatchError, DispatchResult, traits::Dispatchable};

mod tests;
mod benchmarking;

/// Configuration trait.
pub trait Trait: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;

	/// The overarching call type.
	type Call: Parameter + Dispatchable<Origin=Self::Origin, PostInfo=PostDispatchInfo>
		+ GetDispatchInfo + From<frame_system::Call<Self>>
		+ UnfilteredDispatchable<Origin=Self::Origin>;
}

decl_storage! {
	trait Store for Module<T: Trait> as Utility {}
}

decl_event! {
	/// Events type.
	pub enum Event {
		/// Batch of dispatches did not complete fully. Index of first failing dispatch given, as
		/// well as the error.
		BatchInterrupted(u32, DispatchError),
		/// Batch of dispatches completed fully with no error.
		BatchCompleted,
	}
}

/// A module identifier. These are per module and should be stored in a registry somewhere.
#[derive(Clone, Copy, Eq, PartialEq, Encode, Decode)]
struct IndexedUtilityModuleId(u16);

impl TypeId for IndexedUtilityModuleId {
	const TYPE_ID: [u8; 4] = *b"suba";
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;

		/// Send a batch of dispatch calls.
		///
		/// May be called from any origin.
		///
		/// - `calls`: The calls to be dispatched from the same origin.
		///
		/// If origin is root then call are dispatch without checking origin filter. (This includes
		/// bypassing `frame_system::Trait::BaseCallFilter`).
		///
		/// # <weight>
		/// - Base weight: 14.39 + .987 * c µs
		/// - Plus the sum of the weights of the `calls`.
		/// - Plus one additional event. (repeat read/write)
		/// # </weight>
		///
		/// This will return `Ok` in all circumstances. To determine the success of the batch, an
		/// event is deposited. If a call failed and the batch was interrupted, then the
		/// `BatchInterrupted` event is deposited, along with the number of successful calls made
		/// and the error of the failed call. If all were successful, then the `BatchCompleted`
		/// event is deposited.
		#[weight = (
			calls.iter()
				.map(|call| call.get_dispatch_info().weight)
				.fold(15_000_000, |a: Weight, n| a.saturating_add(n).saturating_add(1_000_000)),
			{
				let all_operational = calls.iter()
					.map(|call| call.get_dispatch_info().class)
					.all(|class| class == DispatchClass::Operational);
				if all_operational {
					DispatchClass::Operational
				} else {
					DispatchClass::Normal
				}
			},
		)]
		fn batch(origin, calls: Vec<<T as Trait>::Call>) {
			let is_root = ensure_root(origin.clone()).is_ok();
			for (index, call) in calls.into_iter().enumerate() {
				let result = if is_root {
					call.dispatch_bypass_filter(origin.clone())
				} else {
					call.dispatch(origin.clone())
				};
				if let Err(e) = result {
					Self::deposit_event(Event::BatchInterrupted(index as u32, e.error));
					return Ok(());
				}
			}
			Self::deposit_event(Event::BatchCompleted);
		}

		/// Send a call through an indexed pseudonym of the sender.
		///
		/// Filter from origin are passed along. The call will be dispatched with an origin which
		/// use the same filter as the origin of this call.
		///
		/// NOTE: If you need to ensure that any account-based filtering is not honored (i.e.
		/// because you expect `proxy` to have been used prior in the call stack and you do not want
		/// the call restrictions to apply to any sub-accounts), then use `as_multi_threshold_1`
		/// in the Multisig pallet instead.
		///
		/// NOTE: Prior to version *12, this was called `as_limited_sub`.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// # <weight>
		/// - Base weight: 2.861 µs
		/// - Plus the weight of the `call`
		/// # </weight>
		#[weight = (
			call.get_dispatch_info().weight.saturating_add(3_000_000),
			call.get_dispatch_info().class,
		)]
		fn as_derivative(origin, index: u16, call: Box<<T as Trait>::Call>) -> DispatchResult {
			let mut origin = origin;
			let who = ensure_signed(origin.clone())?;
			let pseudonym = Self::derivative_account_id(who, index);
			origin.set_caller_from(frame_system::RawOrigin::Signed(pseudonym));
			call.dispatch(origin).map(|_| ()).map_err(|e| e.error)
		}
	}
}

impl<T: Trait> Module<T> {
	/// Derive a derivative account ID from the owner account and the sub-account index.
	pub fn derivative_account_id(who: T::AccountId, index: u16) -> T::AccountId {
		let entropy = (b"modlpy/utilisuba", who, index).using_encoded(blake2_256);
		T::AccountId::decode(&mut &entropy[..]).unwrap_or_default()
	}
}
