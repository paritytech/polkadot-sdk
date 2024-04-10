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

//! Meta Tx
//!
//! TODO docs

#![cfg_attr(not(feature = "std"), not_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub use pallet::*;

use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
	pallet_prelude::*,
	traits::OriginTrait,
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::TransactionExtension;
use sp_runtime::traits::TransactionExtensionBase;
use sp_runtime::traits::{Dispatchable, IdentifyAccount, Verify};

/// Meta transaction type.
// TODO: The `MetaTx` type is similar to `sp_runtime::generic::UncheckedExtrinsic`. However,
// `MetaTx` cannot replace generic::UncheckedExtrinsic because we need to box the call type,
// given that `MetaTx` is used as an argument type for a call.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub struct MetaTx<Address, Signature, Call, TxExtension> {
	proof: Proof<Address, Signature>,
	call: Box<Call>,
	tx_ext: TxExtension,
}

impl<Address, Signature, Call, TxExtension> MetaTx<Address, Signature, Call, TxExtension> {
	pub fn new_signed(
		address: Address,
		signature: Signature,
		call: Call,
		tx_ext: TxExtension,
	) -> Self {
		Self { proof: Proof::Signed(address, signature), call: Box::new(call), tx_ext }
	}
}

#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub enum Proof<Address, Signature> {
	Signed(Address, Signature),
	// TODO `General` as in `sp_runtime::generic::Preamble`.
}

pub type MetaTxFor<T> = MetaTx<
	<<T as Config>::PublicKey as IdentifyAccount>::AccountId,
	<T as Config>::Signature,
	<T as Config>::RuntimeCall,
	<T as Config>::TxExtension,
>;

pub type SignedPayloadFor<T> =
	sp_runtime::generic::SignedPayload<<T as Config>::RuntimeCall, <T as Config>::TxExtension>;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The overarching call type.
		type RuntimeCall: Parameter
			+ GetDispatchInfo
			+ Dispatchable<
				Info = DispatchInfo,
				PostInfo = PostDispatchInfo,
				RuntimeOrigin = Self::RuntimeOrigin,
			> + IsType<<Self as frame_system::Config>::RuntimeCall>;
		/// Signature type for meta transactions.
		type Signature: Parameter + Verify<Signer = Self::PublicKey>;
		/// Public key type used for signature verification.
		type PublicKey: IdentifyAccount<AccountId = Self::AccountId>;
		/// The context type of `Self::TxExtension`
		type TxContext: Member + Default;
		/// Transaction extension/s for meta transactions.
		type TxExtension: TransactionExtension<<Self as Config>::RuntimeCall, Self::TxContext>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// TODO
		TODO,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A call was dispatched.
		Dispatched { result: DispatchResultWithPostInfo },
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Dispatch meta transaction.
		///
		/// origin must be signed
		#[pallet::call_index(0)]
		#[pallet::weight({
			let dispatch_info = meta_tx.call.get_dispatch_info();
			// TODO: plus T::WeightInfo::dispatch() which must include the weight of T::TxExtension
			(
				dispatch_info.weight,
				dispatch_info.class,
			)
		})]
		pub fn dispatch(origin: OriginFor<T>, meta_tx: MetaTxFor<T>) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			let (signer, signature) = match meta_tx.proof {
				Proof::Signed(signer, signature) => (signer, signature),
			};

			let signed_payload = SignedPayloadFor::<T>::new(*meta_tx.call, meta_tx.tx_ext)
				.map_err(|_| Error::<T>::TODO)?;

			if !signed_payload.using_encoded(|payload| signature.verify(payload, &signer)) {
				return Err(Error::<T>::TODO.into());
			}

			let origin = T::RuntimeOrigin::signed(signer);
			let (call, tx_ext, _) = signed_payload.deconstruct();
			let info = call.get_dispatch_info();
			// TODO: to get the len we have to encode the original `meta_tx`.
			let len = 0;
			let mut ctx = T::TxContext::default();

			let (_, val, origin) = T::TxExtension::validate(
				&tx_ext,
				origin,
				&call,
				&info,
				len,
				&mut ctx,
				tx_ext.implicit().map_err(|_| Error::<T>::TODO)?,
				&call,
			)
			.map_err(|_| Error::<T>::TODO)?;

			let pre = T::TxExtension::prepare(tx_ext, val, &origin, &call, &info, len, &ctx)
				.map_err(|_| Error::<T>::TODO)?;

			let res = call.dispatch(origin);
			let post_info = res.unwrap_or_else(|err| err.post_info);
			let pd_res = res.map(|_| ()).map_err(|e| e.error);

			T::TxExtension::post_dispatch(pre, &info, &post_info, len, &pd_res, &ctx)
				.map_err(|_| Error::<T>::TODO)?;

			Self::deposit_event(Event::Dispatched { result: res });

			res
		}
	}
}
