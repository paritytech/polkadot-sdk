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

//! # Meta Tx (Meta Transaction) Pallet
//!
//! This pallet enables the dispatch of transactions that are authorized by one party (the signer)
//! and executed by an untrusted third party (the relayer), who covers the transaction fees.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The pallet provides a client-level API, typically not meant for direct use by end users.
//! A meta transaction, constructed with the help of a wallet, contains a target call, necessary
//! extensions, and the signer's signature. This transaction is then broadcast, and any interested
//! relayer can pick it up and execute it. The relayer submits a regular transaction via the
//! [`dispatch`](`Pallet::dispatch`) function, passing the meta transaction as an argument to
//! execute the target call on behalf of the signer while covering the fees.
//!
//! ### Example
#![doc = docify::embed!("src/tests.rs", sign_and_execute_meta_tx)]
//!
//! ## Low-Level / Implementation Details
//!
//! The structure of a meta transaction is identical to a regular transaction. It includes the
//! signer's address, signature, target call, and a configurable set of extensions. The signed
//! payload consists of the call, extensions, and any implicit data required by the extensions.
//! This payload can be represented using the [`sp_runtime::generic::SignedPayload`] type. The
//! extensions follow the same [`TransactionExtension`] contract, and common types such as
//! [`frame_system::CheckGenesis`], [`frame_system::CheckMortality`], [`frame_system::CheckNonce`],
//! etc., are applicable in the context of meta transactions.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub use pallet::*;

use core::ops::Add;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
	pallet_prelude::*,
	traits::OriginTrait,
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{
	AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable, IdentifyAccount,
	TransactionExtension, Verify,
};
use sp_std::prelude::*;

/// Meta Transaction type.
///
/// The data that is provided and signed by the signer and shared with the relayer.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub struct MetaTx<Address, Signature, Call, Extension> {
	/// Information regarding the type of the meta transaction.
	preamble: Preamble<Address, Signature, Extension>,
	/// The target call to be executed on behalf of the signer.
	call: Call,
}

impl<Address, Signature, Call, Extension> MetaTx<Address, Signature, Call, Extension> {
	/// Create a new meta transaction.
	pub fn new_signed(
		address: Address,
		signature: Signature,
		extension: Extension,
		call: Call,
	) -> Self {
		Self { preamble: Preamble::Signed(address, signature, extension), call }
	}

	/// Get the extension reference of the meta transaction.
	pub fn extension_as_ref(&self) -> &Extension {
		match &self.preamble {
			Preamble::Signed(_, _, extension) => extension,
		}
	}
}

/// Proof of the authenticity of the meta transaction.
///
/// It could potentially be extended to support other type of meta transaction, similar to the
/// [`sp_runtime::generic::Preamble::Bare`]` transaction extrinsic type.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub enum Preamble<Address, Signature, Extension> {
	/// Meta transaction that contains the signature, signer's address and the extension with it's
	/// version.
	Signed(Address, Signature, Extension),
}

/// The [`MetaTx`] for the given config.
pub type MetaTxFor<T> = MetaTx<
	<<T as Config>::PublicKey as IdentifyAccount>::AccountId,
	<T as Config>::Signature,
	<T as Config>::RuntimeCall,
	<T as Config>::Extension,
>;

/// The [`sp_runtime::generic::SignedPayload`] for the given config.
pub type SignedPayloadFor<T> =
	sp_runtime::generic::SignedPayload<<T as Config>::RuntimeCall, <T as Config>::Extension>;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config
	where
		Self::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	{
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
		///
		/// The `Signer` of the [`Config::Signature`].
		type PublicKey: IdentifyAccount<AccountId = Self::AccountId>;
		/// Transaction extension/s for meta transactions.
		///
		/// The extensions that must be present in every meta transaction. This
		/// generally includes extensions like [frame_system::CheckSpecVersion],
		/// [frame_system::CheckTxVersion], [frame_system::CheckGenesis],
		/// [frame_system::CheckMortality], [frame_system::CheckNonce], etc.
		type Extension: TransactionExtension<<Self as Config>::RuntimeCall>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid proof (e.g. signature).
		BadProof,
		/// The meta transaction is not yet valid (e.g. nonce too high).
		Future,
		/// The meta transaction is outdated (e.g. nonce too low).
		Stale,
		/// The meta transactions's birth block is ancient.
		AncientBirthBlock,
		/// The meta transaction is invalid.
		Invalid,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config>
	where
		T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	{
		/// A meta transaction has been dispatched.
		///
		/// Contains the dispatch result of the meta transaction along with post-dispatch
		/// information.
		Dispatched { result: DispatchResultWithPostInfo },
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	{
		/// Dispatch a given meta transaction.
		///
		/// - `_origin`: Can be any kind of origin.
		/// - `meta_tx`: Meta Transaction with a target call to be dispatched.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let dispatch_info = meta_tx.call.get_dispatch_info();
			let extension_weight = meta_tx.extension_as_ref().weight(&meta_tx.call);
			// TODO: + dispatch weight
			(
				dispatch_info.call_weight.add(extension_weight),
				dispatch_info.class,
			)
		})]
		pub fn dispatch(
			_origin: OriginFor<T>,
			meta_tx: Box<MetaTxFor<T>>,
		) -> DispatchResultWithPostInfo {
			let meta_tx_size = meta_tx.encoded_size();

			let (signer, signature, extension) = match meta_tx.preamble {
				Preamble::Signed(signer, signature, extension) => (signer, signature, extension),
			};

			let signed_payload = SignedPayloadFor::<T>::new(meta_tx.call, extension)
				.map_err(|_| Error::<T>::Invalid)?;

			if !signed_payload.using_encoded(|payload| signature.verify(payload, &signer)) {
				return Err(Error::<T>::BadProof.into());
			}

			let origin = T::RuntimeOrigin::signed(signer);
			let (call, extension, _) = signed_payload.deconstruct();
			// `info` with worst-case call weight and extension weight.
			let info = {
				let mut info = call.get_dispatch_info();
				info.extension_weight = extension.weight(&call);
				info
			};

			// dispatch the meta transaction.
			let meta_dispatch_res = extension
				.dispatch_transaction(origin, call, &info, meta_tx_size)
				.map_err(Error::<T>::from)?;

			Self::deposit_event(Event::Dispatched { result: meta_dispatch_res });

			// meta weight after possible refunds.
			let meta_weight = meta_dispatch_res
				.map_or_else(|err| err.post_info.actual_weight, |info| info.actual_weight)
				.unwrap_or(info.total_weight());

			// // TODO: post_info + T::WeightInfo::dispatch_weight_without_call_and_ext_weight()
			let dispatch_weight = Weight::from_all(1);

			Ok((Some(dispatch_weight + meta_weight), true.into()).into())
		}
	}

	/// Implements [`From<TransactionValidityError>`] for [`Error`] by mapping the relevant error
	/// variants.
	impl<T> From<TransactionValidityError> for Error<T> {
		fn from(err: TransactionValidityError) -> Self {
			use TransactionValidityError::*;
			match err {
				Unknown(_) => Error::<T>::Invalid,
				Invalid(err) => match err {
					InvalidTransaction::BadProof => Error::<T>::BadProof,
					InvalidTransaction::Future => Error::<T>::Future,
					InvalidTransaction::Stale => Error::<T>::Stale,
					InvalidTransaction::AncientBirthBlock => Error::<T>::AncientBirthBlock,
					_ => Error::<T>::Invalid,
				},
			}
		}
	}
}
