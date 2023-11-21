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
	traits::{
		DispatchInfoOf, Dispatchable, IdentifyAccount, PostDispatchInfoOf, TransactionExtension,
		Verify,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	DispatchResult,
};
use sp_std::fmt::Debug;

#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[scale_info(skip_type_params(Call))]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct VerifyMultiSignature<V>
where
	V: Verify + StaticTypeInfo + Codec + Clone + Eq + PartialEq + Debug,
	<V::Signer as IdentifyAccount>::AccountId:
		StaticTypeInfo + Codec + Clone + Eq + PartialEq + Debug,
{
	signature: V,
	account: <V::Signer as IdentifyAccount>::AccountId,
}

impl<V, Call: Dispatchable + Encode> TransactionExtension<Call> for VerifyMultiSignature<V>
where
	V: Send + Sync + Verify + TypeInfo + Codec + Clone + Eq + PartialEq + StaticTypeInfo + Debug,
	<V::Signer as IdentifyAccount>::AccountId:
		Send + Sync + Clone + TypeInfo + Codec + Clone + Eq + PartialEq + StaticTypeInfo + Debug,
	<Call as Dispatchable>::RuntimeOrigin: From<Option<<V::Signer as IdentifyAccount>::AccountId>>,
{
	const IDENTIFIER: &'static str = "VerifyMultiSignature";
	type Val = ();
	type Pre = ();
	type Implicit = ();
	fn implicit(&self) -> sp_std::result::Result<Self::Implicit, TransactionValidityError> {
		Ok(())
	}

	fn validate(
		&self,
		_origin: <Call as Dispatchable>::RuntimeOrigin,
		call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		target: &[u8],
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let mut msg = call.encode();
		msg.extend_from_slice(target);
		let msg = blake2_256(&msg);

		if !self.signature.verify(&msg[..], &self.account) {
			Err(InvalidTransaction::BadProof)?
		}
		// We clobber the original origin. Maybe we shuld check that it's none?
		let origin = Some(self.account.clone()).into();
		Ok((ValidTransaction::default(), (), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &<Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}

	fn post_dispatch(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<Call>,
		_post_info: &PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}

/*
impl AdditionalSigned for () {
	type Data = ();
	fn additional_signed(&self) -> sp_std::result::Result<Self::Data, TransactionValidityError> {
		Ok(())
	}
}

impl<C: Dispatchable> TransactionExtension for () {
	const IDENTIFIER: &'static str = "...";
	type Call = ...;
	type Val = ();
	type Pre = ();

	fn validate(
		&self,
		origin: <Self::Call as Dispatchable>::RuntimeOrigin,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<
		(ValidTransaction, Self::Val, <Self::Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError
	> {
		Ok((ValidTransaction::default(), Self::Val, origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &<Self::Call as Dispatchable>::RuntimeOrigin,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(Self::Pre)
	}

	fn post_dispatch(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<Self::Call>,
		_post_info: &PostDispatchInfoOf<Self::Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}
*/
