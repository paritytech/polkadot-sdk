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

//! Generic implementation of an extrinsic that has passed the verification
//! stage.

use codec::Encode;

use crate::{
	traits::{
		self, transaction_extension::TransactionExtension, DispatchInfoOf, DispatchTransaction,
		Dispatchable, MaybeDisplay, Member, PostDispatchInfoOf, ValidateUnsigned,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
};

/// The kind of extrinsic this is, including any fields required of that kind. This is basically
/// the full extrinsic except the `Call`.
#[derive(PartialEq, Eq, Clone, sp_core::RuntimeDebug)]
pub enum ExtrinsicFormat<AccountId, Extension> {
	/// Extrinsic is bare; it must pass either the bare forms of `TransactionExtension` or
	/// `ValidateUnsigned`, both deprecated, or alternatively a `ProvideInherent`.
	Bare,
	/// Extrinsic has a default `Origin` of `Signed(AccountId)` and must pass all
	/// `TransactionExtension`s regular checks and includes all extension data.
	Signed(AccountId, Extension),
	/// Extrinsic has a default `Origin` of `None` and must pass all `TransactionExtension`s.
	/// regular checks and includes all extension data.
	General(Extension),
}

// TODO: Rename ValidateUnsigned to ValidateInherent
// TODO: Consider changing ValidateInherent API to avoid need for duplicating validate
//   code into pre_dispatch (rename that to `prepare`).
// TODO: New extrinsic type corresponding to `ExtrinsicFormat::General`, which is
//   unsigned but includes extension data.
// TODO: Move usage of `signed` to `format`:
// - Inherent instead of None.
// - Signed(id, extension) instead of Some((id, extra)).
// - Introduce General(extension) for one without a signature.

/// Definition of something that the external world might want to say; its existence implies that it
/// has been checked and is good, particularly with regards to the signature.
///
/// This is typically passed into [`traits::Applyable::apply`], which should execute
/// [`CheckedExtrinsic::function`], alongside all other bits and bobs.
#[derive(PartialEq, Eq, Clone, sp_core::RuntimeDebug)]
pub struct CheckedExtrinsic<AccountId, Call, Extension> {
	/// Who this purports to be from and the number of extrinsics have come before
	/// from the same signer, if anyone (note this is not a signature).
	pub format: ExtrinsicFormat<AccountId, Extension>,

	/// The function that should be called.
	pub function: Call,
}

impl<AccountId, Call, Extension, RuntimeOrigin> traits::Applyable
	for CheckedExtrinsic<AccountId, Call, Extension>
where
	AccountId: Member + MaybeDisplay,
	Call: Member + Dispatchable<RuntimeOrigin = RuntimeOrigin> + Encode,
	Extension: TransactionExtension<Call, ()>,
	RuntimeOrigin: From<Option<AccountId>>,
{
	type Call = Call;

	fn validate<I: ValidateUnsigned<Call = Self::Call>>(
		&self,
		source: TransactionSource,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> TransactionValidity {
		match self.format {
			ExtrinsicFormat::Bare => {
				let inherent_validation = I::validate_unsigned(source, &self.function)?;
				#[allow(deprecated)]
				let legacy_validation = Extension::validate_bare_compat(&self.function, info, len)?;
				Ok(legacy_validation.combine_with(inherent_validation))
			},
			ExtrinsicFormat::Signed(ref signer, ref extension) => {
				let origin = Some(signer.clone()).into();
				extension.validate_only(origin, &self.function, info, len).map(|x| x.0)
			},
			ExtrinsicFormat::General(ref extension) =>
				extension.validate_only(None.into(), &self.function, info, len).map(|x| x.0),
		}
	}

	fn apply<I: ValidateUnsigned<Call = Self::Call>>(
		self,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Self::Call>> {
		match self.format {
			ExtrinsicFormat::Bare => {
				I::pre_dispatch(&self.function)?;
				// TODO: Remove below once `pre_dispatch_unsigned` is removed from `LegacyExtension`
				//   or `LegacyExtension` is removed.
				#[allow(deprecated)]
				Extension::validate_bare_compat(&self.function, info, len)?;
				#[allow(deprecated)]
				Extension::pre_dispatch_bare_compat(&self.function, info, len)?;
				let res = self.function.dispatch(None.into());
				let post_info = res.unwrap_or_else(|err| err.post_info);
				let pd_res = res.map(|_| ()).map_err(|e| e.error);
				// TODO: Remove below once `pre_dispatch_unsigned` is removed from `LegacyExtension`
				//   or `LegacyExtension` is removed.
				#[allow(deprecated)]
				Extension::post_dispatch_bare_compat(info, &post_info, len, &pd_res)?;
				Ok(res)
			},
			ExtrinsicFormat::Signed(signer, extension) =>
				extension.dispatch_transaction(Some(signer).into(), self.function, info, len),
			ExtrinsicFormat::General(extension) =>
				extension.dispatch_transaction(None.into(), self.function, info, len),
		}
	}
}
