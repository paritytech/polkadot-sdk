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

use super::oracle::Call as OracleCall;
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::DispatchInfo, pallet_prelude::TransactionSource, traits::IsSubType, weights::Weight,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, PostDispatchInfoOf,
		TransactionExtension, ValidateResult,
	},
	transaction_validity::{TransactionPriority, TransactionValidityError, ValidTransaction},
	DispatchResult, SaturatedConversion,
};

/// Transaction extension that extracts the `produced_in` field from the call body
/// and sets it as the transaction priority.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SetPriorityFromProducedIn<T: super::oracle::Config>(core::marker::PhantomData<T>);

impl<T: super::oracle::Config> Default for SetPriorityFromProducedIn<T> {
	fn default() -> Self {
		Self(core::marker::PhantomData)
	}
}

impl<T: super::oracle::Config> core::fmt::Debug for SetPriorityFromProducedIn<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "SetPriorityFromProducedIn")
	}
}

impl<T> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for SetPriorityFromProducedIn<T>
where
	T: super::oracle::Config + frame_system::Config + Send + Sync,
	<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin:
		AsSystemOriginSigner<T::AccountId> + Clone,
	<T as frame_system::Config>::RuntimeCall: IsSubType<OracleCall<T>>,
{
	const IDENTIFIER: &'static str = "SetPriorityFromProducedIn";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn weight(&self, _call: &<T as frame_system::Config>::RuntimeCall) -> Weight {
		// Minimal weight as this is just reading from the call
		Weight::from_parts(1_000, 0)
	}

	fn validate(
		&self,
		origin: <T as frame_system::Config>::RuntimeOrigin,
		call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
		let mut priority: TransactionPriority = 0;

		// Check if our call `IsSubType` of the `RuntimeCall`
		if let Some(OracleCall::vote { produced_in, .. }) = call.is_sub_type() {
			priority = (*produced_in).saturated_into();
		} else {
			log::warn!(target: "runtime::price-oracle::priority-extension", "Unknown call, not setting priority")
		}

		let validity = ValidTransaction { priority, ..Default::default() };

		Ok((validity, (), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &<T as frame_system::Config>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}

	fn post_dispatch_details(
		_pre: Self::Pre,
		_info: &DispatchInfo,
		_post_info: &PostDispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		Ok(Weight::zero())
	}
}
