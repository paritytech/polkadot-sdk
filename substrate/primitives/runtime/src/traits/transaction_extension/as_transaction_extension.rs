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

//! The [AsTransactionExtension] adapter struct for adapting [SignedExtension]s to
//! [TransactionExtension]s.

#![allow(deprecated)]

use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

use crate::{
	traits::{AsSystemOriginSigner, SignedExtension, ValidateResult},
	InvalidTransaction,
};

use super::*;

/// Adapter to use a `SignedExtension` in the place of a `TransactionExtension`.
#[derive(TypeInfo, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
#[deprecated = "Convert your SignedExtension to a TransactionExtension."]
pub struct AsTransactionExtension<SE: SignedExtension>(pub SE);

impl<SE: SignedExtension + Default> Default for AsTransactionExtension<SE> {
	fn default() -> Self {
		Self(SE::default())
	}
}

impl<SE: SignedExtension> From<SE> for AsTransactionExtension<SE> {
	fn from(value: SE) -> Self {
		Self(value)
	}
}

impl<SE: SignedExtension> TransactionExtensionBase for AsTransactionExtension<SE> {
	const IDENTIFIER: &'static str = SE::IDENTIFIER;
	type Implicit = SE::AdditionalSigned;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.additional_signed()
	}
	fn metadata() -> Vec<TransactionExtensionMetadata> {
		SE::metadata()
	}
}

impl<SE: SignedExtension, Context> TransactionExtension<SE::Call, Context>
	for AsTransactionExtension<SE>
where
	<SE::Call as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<SE::AccountId> + Clone,
{
	type Val = ();
	type Pre = SE::Pre;

	fn validate(
		&self,
		origin: <SE::Call as Dispatchable>::RuntimeOrigin,
		call: &SE::Call,
		info: &DispatchInfoOf<SE::Call>,
		len: usize,
		_context: &mut Context,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, SE::Call> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
		let r = self.0.validate(who, call, info, len)?;
		Ok((r, (), origin))
	}

	fn prepare(
		self,
		_: (),
		origin: &<SE::Call as Dispatchable>::RuntimeOrigin,
		call: &SE::Call,
		info: &DispatchInfoOf<SE::Call>,
		len: usize,
		_context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
		self.0.pre_dispatch(who, call, info, len)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<SE::Call>,
		post_info: &PostDispatchInfoOf<SE::Call>,
		len: usize,
		result: &DispatchResult,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		SE::post_dispatch(Some(pre), info, post_info, len, result)
	}

	fn validate_bare_compat(
		call: &SE::Call,
		info: &DispatchInfoOf<SE::Call>,
		len: usize,
	) -> TransactionValidity {
		SE::validate_unsigned(call, info, len)
	}

	fn pre_dispatch_bare_compat(
		call: &SE::Call,
		info: &DispatchInfoOf<SE::Call>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		SE::pre_dispatch_unsigned(call, info, len)
	}

	fn post_dispatch_bare_compat(
		info: &DispatchInfoOf<SE::Call>,
		post_info: &PostDispatchInfoOf<SE::Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		SE::post_dispatch(None, info, post_info, len, result)
	}
}
