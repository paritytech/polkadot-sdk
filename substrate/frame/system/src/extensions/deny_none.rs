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

use crate::Config;
use frame_support::{
	dispatch::DispatchInfo,
	pallet_prelude::{Decode, DispatchResult, Encode, TypeInfo, Weight},
	CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use sp_runtime::{
	traits::{
		transaction_extension::TransactionExtensionBase, Dispatchable, PostDispatchInfoOf,
		TransactionExtension, ValidateResult,
	},
	transaction_validity::TransactionValidityError,
};

#[derive(
	Encode, Decode, CloneNoBound, EqNoBound, PartialEqNoBound, TypeInfo, RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct DenyNone<T>(core::marker::PhantomData<T>);

impl<T: Config + Send + Sync> TransactionExtensionBase for DenyNone<T> {
	const IDENTIFIER: &'static str = "DenyNone";
	type Implicit = ();
	fn weight() -> Weight {
		Weight::from_all(0)
	}
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for DenyNone<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type Val = ();
	type Pre = ();

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfo,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		match origin.into() {
			// TODO TODO: find a better error variant
			Ok(crate::RawOrigin::None) => Err(TransactionValidityError::Invalid(crate::InvalidTransaction::Call)),
			Ok(origin) => Ok((Default::default(), (), origin.into())),
			Err(origin) => Ok((Default::default(), (), origin)),
		}

	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfo,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}

	fn post_dispatch_details(
		_pre: Self::Pre,
		_info: &DispatchInfo,
		_post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Option<Weight>, TransactionValidityError> {
		Ok(None)
	}
}
