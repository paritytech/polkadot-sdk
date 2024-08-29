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
	traits::OriginTrait,
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

impl<T> DenyNone<T> {
	pub fn new() -> Self {
		Self(Default::default())
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for DenyNone<T> {
	const IDENTIFIER: &'static str = "DenyNone";
	type Implicit = ();
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
		if origin.as_system_ref().map_or(false, |system_origin| system_origin.is_none()) {
			// TODO TODO: find a better error variant
			Err(TransactionValidityError::Invalid(crate::InvalidTransaction::Call))
		} else {
			Ok((Default::default(), (), origin))
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
	) -> Result<Weight, TransactionValidityError> {
		Ok(Weight::zero())
	}

	fn weight(&self, _call: &T::RuntimeCall) -> Weight {
		Weight::zero()
	}
}

#[cfg(test)]
mod tests {
	use frame_support::derive_impl;
	use sp_runtime::BuildStorage;
	use sp_runtime::transaction_validity::TransactionValidityError;
	use sp_runtime::transaction_validity::InvalidTransaction;
	use crate as frame_system;
	use frame_support::traits::OriginTrait;
	use sp_runtime::traits::TransactionExtension as _;


	#[frame_support::pallet]
	pub mod pallet1 {
		use crate as frame_system;

		#[pallet::pallet]
		pub struct Pallet<T>(_);

		#[pallet::config]
		pub trait Config: frame_system::Config {}
	}

	#[frame_support::runtime]
	mod runtime {
		#[runtime::runtime]
		#[runtime::derive(
			RuntimeCall,
			RuntimeEvent,
			RuntimeError,
			RuntimeOrigin,
			RuntimeFreezeReason,
			RuntimeHoldReason,
			RuntimeSlashReason,
			RuntimeLockId,
			RuntimeTask
		)]
		pub struct Runtime;

		#[runtime::pallet_index(0)]
		pub type System = frame_system::Pallet<Runtime>;

		#[runtime::pallet_index(1)]
		pub type Pallet1 = pallet1::Pallet<Runtime>;
	}

	pub type TransactionExtension = (
		frame_system::AuthorizeCall<Runtime>,
	);

	pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
	pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
		u64,
		RuntimeCall,
		sp_runtime::testing::MockU64Signature,
		TransactionExtension,
	>;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = Block;
	}

	impl pallet1::Config for Runtime {}

	pub fn new_test_ext() -> sp_io::TestExternalities {
		let t = RuntimeGenesisConfig {
			..Default::default()
		}
			.build_storage()
			.unwrap();
		t.into()
	}

	#[test]
	fn allowed_origin() {
		new_test_ext().execute_with(|| {
			let ext = frame_system::DenyNone::<Runtime>::new();

			let filtered_call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });

			let origin = {
				let mut o: RuntimeOrigin = crate::Origin::<Runtime>::Signed(42).into();
				let filter_clone = filtered_call.clone();
				o.add_filter(move |call| filter_clone != *call);
				o
			};

			let (_, (), new_origin) = ext.validate(
				origin,
				&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 42 }),
				&crate::DispatchInfo::default(),
				Default::default(),
				(),
				&(),
			)
				.expect("valid");

			assert!(!new_origin.filter_call(&filtered_call));
		});
	}

	#[test]
	fn denied_origin() {
		new_test_ext().execute_with(|| {
			let ext = frame_system::DenyNone::<Runtime>::new();

			let origin: RuntimeOrigin = crate::Origin::<Runtime>::None.into();

			let err = ext.validate(
				origin,
				&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 42 }),
				&crate::DispatchInfo::default(),
				Default::default(),
				(),
				&(),
			)
				.expect_err("invalid");

			assert_eq!(err, TransactionValidityError::Invalid(InvalidTransaction::Call));
		});
	}
}
