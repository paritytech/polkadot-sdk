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

//! # Meta Tx or Meta Transaction pallet.
//!
//! The pallet provides a way to dispatch a transaction authorized by one party (the signer) and
//! executed by an untrusted third party (the relayer) that covers the transaction fees.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The pallet exposes a client level API which usually not meant to be used directly by the end
//! user. Meta transaction constructed with a wallet help will contain a target call, required
//! extensions and a signer signature then will be gossiped with the world and can be picked up by
//! anyone who is interested in relaying the transaction. The relayer will publish a regular
//! transaction with the [`dispatch`](`Pallet::dispatch`) call and the meta transaction as an
//! argument to execute the target call on behalf of the signer and cover the fees.
//!
//! The pallet exposes a client-level API, which is usually not meant to be used directly by the
//! end-user. A meta transaction constructed with a wallet's help will contain a target call,
//! required extensions, and a signer's signature. It will then be shared with the world and can
//! be picked up by anyone interested in relaying the transaction. The relayer will publish a
//! regular transaction with the [`dispatch`](`Pallet::dispatch`) call and the meta transaction as
//! an argument to execute the target call on behalf of the signer and cover the fees.
//!
//! ### Example
#![doc = docify::embed!("src/tests.rs", sign_and_execute_meta_tx)]
//!
//! ## Low Level / Implementation Details
//!
//! The layout of the Meta Transaction is identical to the regular transaction. It contains the
//! signer's address, the signature, the target call, and a configurable set of extensions. The
//! signed payload concatenates the call, the extensions, and the implicit data of the extensions
//! and can be represented as the [sp_runtime::generic::SignedPayload] type. The extensions are
//! presented under the same [TransactionExtension] contract, and types like
//! [frame_system::CheckGenesis], [frame_system::CheckMortality], [frame_system::CheckNonce], etc.,
//! can be used and are generally relevant in the context of meta transactions.

#![cfg_attr(not(feature = "std"), no_std)]

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
use sp_runtime::traits::{
	AccountExistenceProvider, Dispatchable, IdentifyAccount, TransactionExtension,
	TransactionExtensionBase, Verify,
};
use sp_std::prelude::*;

/// Meta Transaction type.
///
/// The data that is provided and signed by the signer and shared with the relayer.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub struct MetaTx<Address, Signature, Call, Extension> {
	/// The proof of the authenticity of the meta transaction.
	proof: Proof<Address, Signature>,
	/// The target call to be executed on behalf of the signer.
	call: Box<Call>,
	/// The required extension/s.
	///
	/// This might include the nonce check, expiration, etc.
	extension: Extension,
}

impl<Address, Signature, Call, Extension> MetaTx<Address, Signature, Call, Extension> {
	/// Create a new meta transaction.
	pub fn new_signed(
		address: Address,
		signature: Signature,
		call: Call,
		extension: Extension,
	) -> Self {
		Self { proof: Proof::Signed(address, signature), call: Box::new(call), extension }
	}
}

/// Proof of the authenticity of the meta transaction.
// It could potentially be extended to support additional types of proofs, similar to the
// sp_runtime::generic::Preamble::Bare transaction type.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, Clone, RuntimeDebug)]
pub enum Proof<Address, Signature> {
	/// Signature of the meta transaction payload and the signer's address.
	Signed(Address, Signature),
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
		/// The context type of `Self::Extension`.
		type Context: Member + Default;
		/// Transaction extension/s for meta transactions.
		///
		/// The extensions that must be present in every meta transaction. This
		/// generally includes extensions like [frame_system::CheckSpecVersion],
		/// [frame_system::CheckTxVersion], [frame_system::CheckGenesis],
		/// [frame_system::CheckMortality], [frame_system::CheckNonce], etc.
		type Extension: TransactionExtension<<Self as Config>::RuntimeCall, Self::Context>;
		/// Type to provide for new, nonexistent accounts.
		type ExistenceProvider: AccountExistenceProvider<Self::AccountId>;
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
	pub enum Event<T: Config> {
		/// A call was dispatched.
		Dispatched { result: DispatchResultWithPostInfo },
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Dispatch a given meta transaction.
		///
		/// - `_origin`: Can be any kind of origin.
		/// - `meta_tx`: Meta Transaction with a target call to be dispatched.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let dispatch_info = meta_tx.call.get_dispatch_info();
			// TODO: plus T::WeightInfo::dispatch() which must include the weight of T::Extension
			(
				dispatch_info.weight,
				dispatch_info.class,
			)
		})]
		pub fn dispatch(
			_origin: OriginFor<T>,
			meta_tx: MetaTxFor<T>,
		) -> DispatchResultWithPostInfo {
			let meta_tx_size = meta_tx.encoded_size();

			let (signer, signature) = match meta_tx.proof {
				Proof::Signed(signer, signature) => (signer, signature),
			};

			let signed_payload = SignedPayloadFor::<T>::new(*meta_tx.call, meta_tx.extension)
				.map_err(|_| Error::<T>::Invalid)?;

			if !signed_payload.using_encoded(|payload| signature.verify(payload, &signer)) {
				return Err(Error::<T>::BadProof.into());
			}

			let origin = T::RuntimeOrigin::signed(signer);
			let (call, extension, _) = signed_payload.deconstruct();
			let info = call.get_dispatch_info();
			let mut ctx = T::Context::default();

			let (_, val, origin) = T::Extension::validate(
				&extension,
				origin,
				&call,
				&info,
				meta_tx_size,
				&mut ctx,
				extension.implicit().map_err(|_| Error::<T>::Invalid)?,
				&call,
			)
			.map_err(Error::<T>::from)?;

			let pre =
				T::Extension::prepare(extension, val, &origin, &call, &info, meta_tx_size, &ctx)
					.map_err(Error::<T>::from)?;

			let res = call.dispatch(origin);
			let post_info = res.unwrap_or_else(|err| err.post_info);
			let pd_res = res.map(|_| ()).map_err(|e| e.error);

			T::Extension::post_dispatch(pre, &info, &post_info, meta_tx_size, &pd_res, &ctx)
				.map_err(Error::<T>::from)?;

			Self::deposit_event(Event::Dispatched { result: res });

			res
		}

		/// Dispatch a given meta transaction.
		///
		/// - `origin`: Can be any kind of origin.
		/// - `meta_tx`: Meta Transaction with a target call to be dispatched.
		#[pallet::call_index(1)]
		#[pallet::weight({
			let dispatch_info = meta_tx.call.get_dispatch_info();
			// TODO: plus T::WeightInfo::dispatch() which must include the weight of T::Extension
			(
				dispatch_info.weight,
				dispatch_info.class,
			)
		})]
		pub fn dispatch_creating(
			origin: OriginFor<T>,
			meta_tx: MetaTxFor<T>,
		) -> DispatchResultWithPostInfo {
			let sponsor = ensure_signed(origin)?;
			let meta_tx_size = meta_tx.encoded_size();

			let (signer, signature) = match meta_tx.proof {
				Proof::Signed(signer, signature) => (signer, signature),
			};

			let signed_payload = SignedPayloadFor::<T>::new(*meta_tx.call, meta_tx.extension)
				.map_err(|_| Error::<T>::Invalid)?;

			if !signed_payload.using_encoded(|payload| signature.verify(payload, &signer)) {
				return Err(Error::<T>::BadProof.into());
			}

			if !<frame_system::Pallet<T>>::account_exists(&signer) {
				T::ExistenceProvider::provide(&sponsor, &signer)?;
			}

			let origin = T::RuntimeOrigin::signed(signer);
			let (call, extension, _) = signed_payload.deconstruct();
			let info = call.get_dispatch_info();
			let mut ctx = T::Context::default();

			let (_, val, origin) = T::Extension::validate(
				&extension,
				origin,
				&call,
				&info,
				meta_tx_size,
				&mut ctx,
				extension.implicit().map_err(|_| Error::<T>::Invalid)?,
				&call,
			)
			.map_err(Error::<T>::from)?;

			let pre =
				T::Extension::prepare(extension, val, &origin, &call, &info, meta_tx_size, &ctx)
					.map_err(Error::<T>::from)?;

			let res = call.dispatch(origin);
			let post_info = res.unwrap_or_else(|err| err.post_info);
			let pd_res = res.map(|_| ()).map_err(|e| e.error);

			T::Extension::post_dispatch(pre, &info, &post_info, meta_tx_size, &pd_res, &ctx)
				.map_err(Error::<T>::from)?;

			Self::deposit_event(Event::Dispatched { result: res });

			res
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
