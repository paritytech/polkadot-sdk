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

//! Transaction extensions.

use crate::{CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use codec::{Codec, Decode, Encode};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_io::hashing::blake2_256;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		transaction_extension::TransactionExtension, AsAuthorizedOrigin, DispatchInfoOf,
		Dispatchable, IdentifyAccount, Verify,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};
use sp_std::fmt::Debug;
use sp_weights::Weight;

/// Extension that, if enabled, validates a signature type against the payload constructed from the
/// call and the rest of the transaction extension pipeline. This extension provides the
/// functionality that traditionally signed transactions had with the implicit signature checking
/// implemented in [`Checkable`](sp_runtime::traits::Checkable). It is meant to be placed ahead of
/// any other extensions that do authorization work in the [`TransactionExtension`] pipeline.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub enum VerifyMultiSignature<V: Verify>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	/// The extension will verify the signature and, if successful, authorize a traditionally
	/// signed transaction.
	Signed {
		/// The signature provided by the transaction submitter.
		signature: V,
		/// The account that signed the payload.
		account: <V::Signer as IdentifyAccount>::AccountId,
	},
	/// The extension is disabled and will be passthrough.
	Disabled,
}

impl<V: Verify> VerifyMultiSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	/// Create a new extension instance that will validate the provided signature.
	pub fn new_with_signature(
		signature: V,
		account: <V::Signer as IdentifyAccount>::AccountId,
	) -> Self {
		Self::Signed { signature, account }
	}

	/// Create a new passthrough extension instance.
	pub fn new_disabled() -> Self {
		Self::Disabled
	}
}

impl<V: Verify, Call: Dispatchable + Encode> TransactionExtension<Call> for VerifyMultiSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<Call as Dispatchable>::RuntimeOrigin:
		From<Option<<V::Signer as IdentifyAccount>::AccountId>> + AsAuthorizedOrigin,
{
	const IDENTIFIER: &'static str = "VerifyMultiSignature";
	type Implicit = ();
	type Val = ();
	type Pre = ();
	impl_tx_ext_default!(Call; prepare);

	fn weight(&self, _call: &Call) -> Weight {
		match &self {
			// The benchmarked weight of the payload construction and signature checking.
			Self::Signed { .. } => {
				// TODO: create a pallet to benchmark this weight.
				Weight::zero()
			},
			// When the extension is passthrough, it consumes no weight.
			Self::Disabled => Weight::zero(),
		}
	}

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_: (),
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		// If the extension is disabled, return early.
		let Self::Signed { signature, account } = &self else {
			return Ok((Default::default(), (), origin))
		};

		// This extension must receive an unauthorized origin as it is meant to headline the
		// authorization extension pipeline. Any extensions that precede this one must not authorize
		// any origin and serve some other functional purpose.
		if origin.is_authorized() {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// Construct the payload that the signature will be validated against. The inherited
		// implication contains the encoded bytes of the call and all of the extension data of the
		// extensions that follow in the `TransactionExtension` pipeline.
		//
		// In other words:
		// - extensions that precede this extension are ignored in terms of signature validation;
		// - extensions that follow this extension are included in the payload to be signed (as if
		//   they were the entire `SignedExtension` pipeline in the traditional signed transaction
		//   model).
		//
		// The encoded bytes of the payload are then hashed using `blake2_256`.
		let msg = inherited_implication.using_encoded(blake2_256);

		// The extension was enabled, so the signature must match.
		if !signature.verify(&msg[..], account) {
			Err(InvalidTransaction::BadProof)?
		}

		// Return the signer as the transaction origin.
		let origin = Some(account.clone()).into();
		Ok((ValidTransaction::default(), (), origin))
	}
}
