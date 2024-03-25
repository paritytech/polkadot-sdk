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

/// Transaction extension that sets the origin to the given account ID if the provided signature by
/// that account is valid for all subsequent extensions. If signature is not provided, this
/// extension is no-op. Will run wrapped extension logic after the origin validation.
// TODO better doc.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct SignedOriginSignature<V: Verify, InnerTx>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	InnerTx: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub signature: Option<(V, <V::Signer as IdentifyAccount>::AccountId)>,
	pub extension: InnerTx,
}

impl<V: Verify, InnerTx> SignedOriginSignature<V, InnerTx>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	InnerTx: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub fn new_with_sign(
		signature: V,
		account_id: <V::Signer as IdentifyAccount>::AccountId,
		extension: InnerTx,
	) -> Self {
		Self { signature: Some((signature, account_id)), extension }
	}
}

impl<V: Verify, InnerTx> TransactionExtensionBase for SignedOriginSignature<V, InnerTx>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	InnerTx: Codec
		+ Debug
		+ Sync
		+ Send
		+ Clone
		+ Eq
		+ PartialEq
		+ StaticTypeInfo
		+ TransactionExtensionBase,
{
	const IDENTIFIER: &'static str = "SignedOriginSignature";
	type Implicit = ();
}

impl<V: Verify, Call: Dispatchable + Encode + Clone, Context, InnerTx>
	TransactionExtension<Call, Context> for SignedOriginSignature<V, InnerTx>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<Call as Dispatchable>::RuntimeOrigin: From<Option<<V::Signer as IdentifyAccount>::AccountId>>,
	InnerTx: Codec
		+ Debug
		+ Sync
		+ Send
		+ Clone
		+ Eq
		+ PartialEq
		+ StaticTypeInfo
		+ TransactionExtension<Call, Context>,
{
	type Val = InnerTx::Val;
	type Pre = InnerTx::Pre;

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &mut Context,
		_self_implicit: (),
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let (signature, account_id) = match &self.signature {
			Some((s, a)) => (s, a.clone()), // TODO check if origin None
			None => {
				let implicit = self.extension.implicit()?;
				return self.extension.validate(
					origin,
					call,
					info,
					len,
					context,
					implicit,
					inherited_implication,
				)
			},
		};

		let implicit = self.extension.implicit()?;
		let signed_payload = (call, &self.extension, &implicit);
		if !signed_payload
			.using_encoded(|payload| signature.verify(&blake2_256(payload)[..], &account_id))
		{
			return Err(InvalidTransaction::BadProof.into())
		}

		let origin = Some(account_id).into();
		self.extension
			.validate(origin, call, info, len, context, implicit, inherited_implication)
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &sp_runtime::traits::OriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.extension.prepare(val, origin, call, info, len, context)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &sp_runtime::traits::PostDispatchInfoOf<Call>,
		len: usize,
		result: &sp_runtime::DispatchResult,
		context: &Context,
	) -> Result<(), TransactionValidityError> {
		InnerTx::post_dispatch(pre, info, post_info, len, result, context)
	}
}

/// Transaction extension that sets the origin to the given account ID if the provided signature by
/// that account is valid for the provided payload.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct CheckSignedPayload<V: Verify, P: Encode>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	P: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub signature: Option<(V, <V::Signer as IdentifyAccount>::AccountId, P)>,
}

impl<V: Verify, P: Encode> CheckSignedPayload<V, P>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	P: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub fn new_with_sign(
		signature: V,
		account_id: <V::Signer as IdentifyAccount>::AccountId,
		payload: P,
	) -> Self {
		Self { signature: Some((signature, account_id, payload)) }
	}

	pub fn new() -> Self {
		Self { signature: None }
	}
}

impl<V: Verify, P: Encode> TransactionExtensionBase for CheckSignedPayload<V, P>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	P: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	const IDENTIFIER: &'static str = "CheckSignedPayload";
	type Implicit = ();
}

impl<V: Verify, P: Encode, Call: Dispatchable + Encode + Clone, Context>
	TransactionExtension<Call, Context> for CheckSignedPayload<V, P>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	P: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
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
		_context: &mut Context,
		_self_implicit: (),
		_inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let (signature, account_id, payload) = match &self.signature {
			Some((s, a, p)) => (s, a.clone(), p), // TODO check if origin None
			None => return Ok((ValidTransaction::default(), (), origin)),
		};

		if !payload.using_encoded(|payload| signature.verify(&blake2_256(payload)[..], &account_id))
		{
			return Err(InvalidTransaction::BadProof.into())
		}

		let origin = Some(account_id).into();
		Ok((ValidTransaction::default(), (), origin))
	}
}
