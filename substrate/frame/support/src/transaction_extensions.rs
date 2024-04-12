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
		transaction_extension::TransactionExtensionBase, DispatchInfoOf, Dispatchable,
		IdentifyAccount, TransactionExtension, Verify,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};
use sp_std::fmt::Debug;

#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct VerifyMultiSignature<V: Verify>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	signature: V,
	account: <V::Signer as IdentifyAccount>::AccountId,
}

impl<V: Verify> TransactionExtensionBase for VerifyMultiSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	const IDENTIFIER: &'static str = "VerifyMultiSignature";
	type Implicit = ();
}

impl<V: Verify, Call: Dispatchable + Encode, Context> TransactionExtension<Call, Context>
	for VerifyMultiSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<Call as Dispatchable>::RuntimeOrigin: From<Option<<V::Signer as IdentifyAccount>::AccountId>>,
{
	type Val = ();
	type Pre = ();
	impl_tx_ext_default!(Call; Context; prepare);

	fn validate(
		&self,
		_origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_: &mut Context,
		_: (),
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let msg = inherited_implication.using_encoded(blake2_256);

		if !self.signature.verify(&msg[..], &self.account) {
			Err(InvalidTransaction::BadProof)?
		}
		// We clobber the original origin. Maybe we shuld check that it's none?
		let origin = Some(self.account.clone()).into();
		Ok((ValidTransaction::default(), (), origin))
	}
}

/// Transaction extension that sets the signed origin of the account ID from a given signature if
/// that signature was derived from the `inherited_implication`, which contains the call and all
/// subsequent extensions. If signature is not provided, this extension is no-op.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct VerifyAccountSignature<V: Verify>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	signature: Option<(V, <V::Signer as IdentifyAccount>::AccountId)>,
}

impl<V: Verify> Default for VerifyAccountSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	fn default() -> Self {
		Self { signature: None }
	}
}

impl<V: Verify> VerifyAccountSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub fn new_with_sign(signature: V, account: <V::Signer as IdentifyAccount>::AccountId) -> Self {
		Self { signature: Some((signature, account)) }
	}
}

impl<V: Verify> TransactionExtensionBase for VerifyAccountSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	const IDENTIFIER: &'static str = "VerifyAccountSignature";
	type Implicit = ();
}

impl<V: Verify, Call: Dispatchable + Encode, Context> TransactionExtension<Call, Context>
	for VerifyAccountSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<Call as Dispatchable>::RuntimeOrigin: From<Option<<V::Signer as IdentifyAccount>::AccountId>>,
{
	type Val = ();
	type Pre = ();
	impl_tx_ext_default!(Call; Context; prepare);

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_: &mut Context,
		_: (),
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let (signature, account_id) = match &self.signature {
			Some((s, a)) => (s, a.clone()),
			None => return Ok((ValidTransaction::default(), (), origin)),
		};

		let msg = inherited_implication.using_encoded(blake2_256);

		if !signature.verify(&msg[..], &account_id) {
			Err(InvalidTransaction::BadProof)?
		}
		let origin = Some(account_id).into();
		Ok((ValidTransaction::default(), (), origin))
	}
}
