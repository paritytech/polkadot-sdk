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

use core::{fmt, marker::PhantomData};

use codec::{Decode, Encode};
use frame_support::{traits::OriginTrait, Parameter};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		DispatchInfoOf, DispatchOriginOf, IdentifyAccount, TransactionExtension, ValidateResult,
		Verify,
	},
	transaction_validity::{InvalidTransaction, ValidTransaction},
};

use crate::pallet_coownership::{Config, Origin};

/// Helper struct to organize the data needed for signature verification of both parties involved.
#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct AuthCredentials<Signer, Signature> {
	first: (Signer, Signature),
	second: (Signer, Signature),
}

/// Extension that, if activated by providing a pair of signers and signatures, will authorize a
/// coowner origin of the two signers. Both signers have to construct their signatures on all of the
/// data that follows this extension in the `TransactionExtension` pipeline, their implications and
/// the call. Essentially re-sign the transaction from this point onwards in the pipeline by using
/// the `inherited_implication`, as shown below.
#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct AuthorizeCoownership<T, Signer, Signature> {
	inner: Option<AuthCredentials<Signer, Signature>>,
	_phantom: PhantomData<T>,
}

impl<T: Config, Signer, Signature> Default for AuthorizeCoownership<T, Signer, Signature> {
	fn default() -> Self {
		Self { inner: None, _phantom: Default::default() }
	}
}

impl<T: Config, Signer, Signature> AuthorizeCoownership<T, Signer, Signature> {
	/// Creates an active extension that will try to authorize the coownership origin.
	pub fn new(first: (Signer, Signature), second: (Signer, Signature)) -> Self {
		Self { inner: Some(AuthCredentials { first, second }), _phantom: Default::default() }
	}
}

impl<T: Config, Signer, Signature> fmt::Debug for AuthorizeCoownership<T, Signer, Signature> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "AuthorizeCoownership")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync, Signer, Signature> TransactionExtension<T::RuntimeCall>
	for AuthorizeCoownership<T, Signer, Signature>
where
	Signer: IdentifyAccount<AccountId = T::AccountId> + Parameter + Send + Sync + 'static,
	Signature: Verify<Signer = Signer> + Parameter + Send + Sync + 'static,
{
	const IDENTIFIER: &'static str = "AuthorizeCoownership";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn validate(
		&self,
		mut origin: DispatchOriginOf<T::RuntimeCall>,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		inherited_implication: &impl codec::Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		// If the extension is inactive, just move on in the pipeline.
		let Some(auth) = &self.inner else {
			return Ok((ValidTransaction::default(), (), origin));
		};
		let first_account = auth.first.0.clone().into_account();
		let second_account = auth.second.0.clone().into_account();

		// Construct the payload to sign using the `inherited_implication`.
		let msg = inherited_implication.using_encoded(sp_io::hashing::blake2_256);

		// Both parties' signatures must be correct for the origin to be authorized.
		// In a prod environment, we're just return a `InvalidTransaction::BadProof` if the
		// signature isn't valid, but we return these custom errors to be able to assert them in
		// tests.
		if !auth.first.1.verify(&msg[..], &first_account) {
			Err(InvalidTransaction::Custom(100))?
		}
		if !auth.second.1.verify(&msg[..], &second_account) {
			Err(InvalidTransaction::Custom(200))?
		}
		// Construct a `pallet_coownership::Origin`.
		let local_origin = Origin::Coowners(first_account, second_account);
		// Turn it into a local `PalletsOrigin`.
		let local_origin = <T as Config>::PalletsOrigin::from(local_origin);
		// Then finally into a pallet `RuntimeOrigin`.
		let local_origin = <T as Config>::RuntimeOrigin::from(local_origin);
		// Which the `set_caller_from` function will convert into the overarching `RuntimeOrigin`
		// created by `construct_runtime!`.
		origin.set_caller_from(local_origin);
		// Make sure to return the new origin.
		Ok((ValidTransaction::default(), (), origin))
	}
	// We're not doing any special logic in `TransactionExtension::prepare`, so just impl a default.
	impl_tx_ext_default!(T::RuntimeCall; weight prepare);
}
