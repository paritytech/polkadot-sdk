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

use crate::{limits::BlockWeights, Config, Pallet, LOG_TARGET};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::TransactionSource,
	traits::Get,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		DispatchInfoOf, Dispatchable, PostDispatchInfoOf, TransactionExtension, ValidateResult,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	DispatchResult,
};
use sp_weights::Weight;

/// Block resource (weight) limit check.
///
/// # Transaction Validity
///
/// This extension does not influence any fields of `TransactionValidity` in case the
/// transaction is valid.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckWeight<T: Config + Send + Sync>(core::marker::PhantomData<T>);

impl<T: Config + Send + Sync> Default for CheckWeight<T> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<T: Config + Send + Sync> CheckWeight<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	/// Checks if the current extrinsic does not exceed the maximum weight a single extrinsic
	/// with given `DispatchClass` can have.
	fn check_extrinsic_weight(
		info: &DispatchInfoOf<T::RuntimeCall>,
	) -> Result<(), TransactionValidityError> {
		let max = T::BlockWeights::get().get(info.class).max_extrinsic;
		let total_weight = info.total_weight();
		match max {
			Some(max) if total_weight.any_gt(max) => {
				log::debug!(
					target: LOG_TARGET,
					"Extrinsic weight {} is greater than the max extrinsic {}",
					total_weight,
					max,
				);

				Err(InvalidTransaction::ExhaustsResources.into())
			},
			_ => Ok(()),
		}
	}

	/// Checks if the current extrinsic can fit into the block with respect to block length limits.
	///
	/// Upon successes, it returns the new block length as a `Result`.
	fn check_block_length(
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<u32, TransactionValidityError> {
		let length_limit = T::BlockLength::get();
		let current_len = Pallet::<T>::block_size();
		let added_len = len as u32;
		let next_len = current_len.saturating_add(added_len);
		if next_len > *length_limit.max.get(info.class) {
			log::debug!(
				target: LOG_TARGET,
				"Exceeded block length limit: {} > {}",
				next_len,
				length_limit.max.get(info.class),
			);

			Err(InvalidTransaction::ExhaustsResources.into())
		} else {
			Ok(next_len)
		}
	}

	/// Creates new `TransactionExtension` to check weight of the extrinsic.
	pub fn new() -> Self {
		Self(Default::default())
	}

	/// Do the validate checks. This can be applied to both signed and unsigned.
	///
	/// It only checks that the block weight and length limit will not exceed.
	///
	/// Returns the transaction validity and the next block length, to be used in `prepare`.
	pub fn do_validate(
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<(ValidTransaction, u32), TransactionValidityError> {
		// If they return `Ok`, then it is below the limit.
		let next_len = Self::check_block_length(info, len)?;
		// during validation we skip block limit check. Since the `validate_transaction`
		// call runs on an empty block anyway, by this we prevent `on_initialize` weight
		// consumption from causing false negatives.
		// Note: the extrinsic length weight is already part of `info.total_weight()`.
		Self::check_extrinsic_weight(info)?;

		Ok((Default::default(), next_len))
	}

	/// Do the pre-dispatch checks. This can be applied to both signed and unsigned.
	///
	/// It checks and notes the new weight and length.
	pub fn do_prepare(
		info: &DispatchInfoOf<T::RuntimeCall>,
		next_len: u32,
	) -> Result<(), TransactionValidityError> {
		let all_weight = Pallet::<T>::block_weight();
		let maximum_weight = T::BlockWeights::get();
		let next_weight =
			calculate_consumed_weight::<T::RuntimeCall>(&maximum_weight, all_weight, info)?;
		// Extrinsic weight already checked in `validate`.

		crate::BlockSize::<T>::put(next_len);
		crate::BlockWeight::<T>::put(next_weight);
		Ok(())
	}

	#[deprecated(note = "Use `frame_system::Pallet::reclaim_weight` instead.")]
	pub fn do_post_dispatch(
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
	) -> Result<(), TransactionValidityError> {
		crate::Pallet::<T>::reclaim_weight(info, post_info)
	}
}

/// Returns the weight that `CheckWeight` books for an extrinsic before dispatch.
///
/// The extrinsic length must already be part of `info` via [`DispatchInfo::length_weight`].
pub fn calculate_consumed_extrinsic_weight<Call>(
	maximum_weight: &BlockWeights,
	info: &DispatchInfoOf<Call>,
) -> Weight
where
	Call: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	info.total_weight()
		.saturating_add(maximum_weight.get(info.class).base_extrinsic)
}

/// Checks if the current extrinsic can fit into the block with respect to block weight limits.
///
/// Upon successes, it returns the new block weight as a `Result`.
pub fn calculate_consumed_weight<Call>(
	maximum_weight: &BlockWeights,
	mut all_weight: crate::ConsumedWeight,
	info: &DispatchInfoOf<Call>,
) -> Result<crate::ConsumedWeight, TransactionValidityError>
where
	Call: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	// Extrinsic length is already in `info.total_weight()` via `DispatchInfo::length_weight`.
	let extrinsic_weight = calculate_consumed_extrinsic_weight::<Call>(maximum_weight, info);
	let limit_per_class = maximum_weight.get(info.class);

	// add the weight. If class is unlimited, use saturating add instead of checked one.
	if limit_per_class.max_total.is_none() && limit_per_class.reserved.is_none() {
		all_weight.accrue(extrinsic_weight, info.class)
	} else {
		all_weight.checked_accrue(extrinsic_weight, info.class).map_err(|_| {
			log::debug!(
				target: LOG_TARGET,
				"All weight checked add overflow.",
			);

			InvalidTransaction::ExhaustsResources
		})?;
	}

	let per_class = *all_weight.get(info.class);

	// Check if we don't exceed per-class allowance
	match limit_per_class.max_total {
		Some(max) if per_class.any_gt(max) => {
			log::debug!(
				target: LOG_TARGET,
				"Exceeded the per-class allowance.",
			);

			return Err(InvalidTransaction::ExhaustsResources.into());
		},
		// There is no `max_total` limit (`None`),
		// or we are below the limit.
		_ => {},
	}

	// In cases total block weight is exceeded, we need to fall back
	// to `reserved` pool if there is any.
	if all_weight.total().any_gt(maximum_weight.max_block) {
		match limit_per_class.reserved {
			// We are over the limit in reserved pool.
			Some(reserved) if per_class.any_gt(reserved) => {
				log::debug!(
					target: LOG_TARGET,
					"Total block weight is exceeded.",
				);

				return Err(InvalidTransaction::ExhaustsResources.into());
			},
			// There is either no limit in reserved pool (`None`),
			// or we are below the limit.
			_ => {},
		}
	}

	Ok(all_weight)
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for CheckWeight<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	const IDENTIFIER: &'static str = "CheckWeight";
	type Implicit = ();
	type Pre = ();
	type Val = u32; // next block length

	fn weight(&self, _: &T::RuntimeCall) -> Weight {
		<T::ExtensionsWeightInfo as super::WeightInfo>::check_weight()
	}

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let (validity, next_len) = Self::do_validate(info, len)?;
		Ok((validity, next_len, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Self::do_prepare(info, val)
	}

	fn post_dispatch_details(
		_pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		crate::Pallet::<T>::reclaim_weight(info, post_info).map(|()| Weight::zero())
	}

	fn bare_validate(
		_call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> frame_support::pallet_prelude::TransactionValidity {
		Ok(Self::do_validate(info, len)?.0)
	}

	fn bare_validate_and_prepare(
		_call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		let (_, next_len) = Self::do_validate(info, len)?;
		Self::do_prepare(info, next_len)
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		crate::Pallet::<T>::reclaim_weight(info, post_info)
	}
}

impl<T: Config + Send + Sync> core::fmt::Debug for CheckWeight<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "CheckWeight")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, RuntimeBlockWeights, System, Test, CALL},
		BlockSize, BlockWeight, DispatchClass,
	};
	use core::marker::PhantomData;
	use frame_support::{assert_err, assert_ok, dispatch::Pays, weights::Weight};
	use sp_runtime::traits::DispatchTransaction;

	fn block_weights() -> crate::limits::BlockWeights {
		<Test as crate::Config>::BlockWeights::get()
	}

	fn normal_weight_limit() -> Weight {
		block_weights()
			.get(DispatchClass::Normal)
			.max_total
			.unwrap_or_else(|| block_weights().max_block)
	}

	fn block_weight_limit() -> Weight {
		block_weights().max_block
	}

	fn normal_length_limit() -> u32 {
		*<Test as Config>::BlockLength::get().max.get(DispatchClass::Normal)
	}

	#[test]
	fn mandatory_extrinsic_doesnt_care_about_limits() {
		fn check(call: impl FnOnce(&DispatchInfo, usize)) {
			new_test_ext().execute_with(|| {
				let max = DispatchInfo {
					call_weight: Weight::MAX,
					class: DispatchClass::Mandatory,
					..Default::default()
				};
				let len = 0_usize;

				call(&max, len);
			});
		}

		check(|max, len| {
			let next_len = CheckWeight::<Test>::check_block_length(max, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(max, next_len));
			assert_eq!(System::block_weight().total(), Weight::MAX);
			assert!(System::block_weight().total().ref_time() > block_weight_limit().ref_time());
		});
		check(|max, len| {
			assert_ok!(CheckWeight::<Test>::do_validate(max, len));
		});
	}

	#[test]
	fn normal_extrinsic_limited_by_maximum_extrinsic_weight() {
		new_test_ext().execute_with(|| {
			let max = DispatchInfo {
				call_weight: block_weights().get(DispatchClass::Normal).max_extrinsic.unwrap() +
					Weight::from_parts(1, 0),
				class: DispatchClass::Normal,
				..Default::default()
			};
			let len = 0_usize;
			assert_err!(
				CheckWeight::<Test>::do_validate(&max, len),
				InvalidTransaction::ExhaustsResources
			);
		});
	}

	#[test]
	fn operational_extrinsic_limited_by_operational_space_limit() {
		new_test_ext().execute_with(|| {
			let weights = block_weights();
			let operational_limit = weights
				.get(DispatchClass::Operational)
				.max_total
				.unwrap_or_else(|| weights.max_block);
			let base_weight = weights.get(DispatchClass::Operational).base_extrinsic;

			let call_weight = operational_limit - base_weight;
			let okay = DispatchInfo {
				call_weight,
				class: DispatchClass::Operational,
				..Default::default()
			};
			let max = DispatchInfo {
				call_weight: call_weight + Weight::from_parts(1, 0),
				class: DispatchClass::Operational,
				..Default::default()
			};
			let len = 0_usize;

			assert_eq!(CheckWeight::<Test>::do_validate(&okay, len), Ok(Default::default()));
			assert_err!(
				CheckWeight::<Test>::do_validate(&max, len),
				InvalidTransaction::ExhaustsResources
			);
		});
	}

	#[test]
	fn register_extra_weight_unchecked_doesnt_care_about_limits() {
		new_test_ext().execute_with(|| {
			System::register_extra_weight_unchecked(Weight::MAX, DispatchClass::Normal);
			assert_eq!(System::block_weight().total(), Weight::MAX);
			assert!(System::block_weight().total().ref_time() > block_weight_limit().ref_time());
		});
	}

	#[test]
	fn full_block_with_normal_and_operational() {
		new_test_ext().execute_with(|| {
			// Max block is 1024
			// Max normal is 768 (75%)
			// 10 is taken for block execution weight
			// So normal extrinsic can be 758 weight (-5 for base extrinsic weight)
			// And Operational can be 246 to produce a full block (-10 for base)
			let max_normal =
				DispatchInfo { call_weight: Weight::from_parts(753, 0), ..Default::default() };
			let rest_operational = DispatchInfo {
				call_weight: Weight::from_parts(246, 0),
				class: DispatchClass::Operational,
				..Default::default()
			};

			let len = 0_usize;

			let next_len = CheckWeight::<Test>::check_block_length(&max_normal, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&max_normal, next_len));
			assert_eq!(System::block_weight().total(), Weight::from_parts(768, 0));
			let next_len = CheckWeight::<Test>::check_block_length(&rest_operational, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&rest_operational, next_len));
			assert_eq!(block_weight_limit(), Weight::from_parts(1024, u64::MAX));
			assert_eq!(System::block_weight().total(), block_weight_limit().set_proof_size(0));
			// Checking single extrinsic should not take current block weight into account.
			assert_eq!(CheckWeight::<Test>::check_extrinsic_weight(&rest_operational), Ok(()));
		});
	}

	#[test]
	fn dispatch_order_does_not_effect_weight_logic() {
		new_test_ext().execute_with(|| {
			// We switch the order of `full_block_with_normal_and_operational`
			let max_normal =
				DispatchInfo { call_weight: Weight::from_parts(753, 0), ..Default::default() };
			let rest_operational = DispatchInfo {
				call_weight: Weight::from_parts(246, 0),
				class: DispatchClass::Operational,
				..Default::default()
			};

			let len = 0_usize;

			let next_len = CheckWeight::<Test>::check_block_length(&rest_operational, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&rest_operational, next_len));
			// Extra 20 here from block execution + base extrinsic weight
			assert_eq!(System::block_weight().total(), Weight::from_parts(266, 0));
			let next_len = CheckWeight::<Test>::check_block_length(&max_normal, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&max_normal, next_len));
			assert_eq!(block_weight_limit(), Weight::from_parts(1024, u64::MAX));
			assert_eq!(System::block_weight().total(), block_weight_limit().set_proof_size(0));
		});
	}

	#[test]
	fn operational_works_on_full_block() {
		new_test_ext().execute_with(|| {
			// An on_initialize takes up the whole block! (Every time!)
			System::register_extra_weight_unchecked(Weight::MAX, DispatchClass::Mandatory);
			let dispatch_normal = DispatchInfo {
				call_weight: Weight::from_parts(251, 0),
				class: DispatchClass::Normal,
				..Default::default()
			};
			let dispatch_operational = DispatchInfo {
				call_weight: Weight::from_parts(246, 0),
				class: DispatchClass::Operational,
				..Default::default()
			};
			let len = 0_usize;

			let next_len = CheckWeight::<Test>::check_block_length(&dispatch_normal, len).unwrap();
			assert_err!(
				CheckWeight::<Test>::do_prepare(&dispatch_normal, next_len),
				InvalidTransaction::ExhaustsResources
			);
			let next_len =
				CheckWeight::<Test>::check_block_length(&dispatch_operational, len).unwrap();
			// Thank goodness we can still do an operational transaction to possibly save the
			// blockchain.
			assert_ok!(CheckWeight::<Test>::do_prepare(&dispatch_operational, next_len));
			// Not too much though
			assert_err!(
				CheckWeight::<Test>::do_prepare(&dispatch_operational, next_len),
				InvalidTransaction::ExhaustsResources
			);
			// Even with full block, validity of single transaction should be correct.
			assert_eq!(CheckWeight::<Test>::check_extrinsic_weight(&dispatch_operational), Ok(()));
		});
	}

	#[test]
	fn signed_ext_check_weight_works_operational_tx() {
		new_test_ext().execute_with(|| {
			let normal =
				DispatchInfo { call_weight: Weight::from_parts(100, 0), ..Default::default() };
			let op = DispatchInfo {
				call_weight: Weight::from_parts(100, 0),
				extension_weight: Weight::zero(),
				length_weight: Weight::zero(),
				class: DispatchClass::Operational,
				pays_fee: Pays::Yes,
			};
			let len = 0_usize;
			let normal_limit = normal_weight_limit();

			// given almost full block
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(normal_limit, DispatchClass::Normal)
			});
			// will not fit.
			assert_eq!(
				CheckWeight::<Test>(PhantomData)
					.validate_and_prepare(Some(1).into(), CALL, &normal, len, 0)
					.unwrap_err(),
				InvalidTransaction::ExhaustsResources.into()
			);
			// will fit.
			assert_ok!(CheckWeight::<Test>(PhantomData).validate_and_prepare(
				Some(1).into(),
				CALL,
				&op,
				len,
				0,
			));

			// likewise for length limit.
			let len = 100_usize;
			BlockSize::<Test>::put(normal_length_limit());
			assert_eq!(
				CheckWeight::<Test>(PhantomData)
					.validate_and_prepare(Some(1).into(), CALL, &normal, len, 0)
					.unwrap_err(),
				InvalidTransaction::ExhaustsResources.into()
			);
			assert_ok!(CheckWeight::<Test>(PhantomData).validate_and_prepare(
				Some(1).into(),
				CALL,
				&op,
				len,
				0,
			));
		})
	}

	#[test]
	fn signed_ext_check_weight_block_size_works() {
		new_test_ext().execute_with(|| {
			let normal = DispatchInfo::default();
			let normal_len_limit = normal_length_limit() as usize;
			let reset_check_weight = |tx, s, f| {
				BlockSize::<Test>::put(0);
				let r = CheckWeight::<Test>(PhantomData).validate_and_prepare(
					Some(1).into(),
					CALL,
					tx,
					s,
					0,
				);
				if f {
					assert!(r.is_err())
				} else {
					assert!(r.is_ok())
				}
			};

			reset_check_weight(&normal, normal_len_limit - 1, false);
			reset_check_weight(&normal, normal_len_limit, false);
			reset_check_weight(&normal, normal_len_limit + 1, true);

			// Operational ones don't have this limit.
			let op = DispatchInfo {
				call_weight: Weight::zero(),
				extension_weight: Weight::zero(),
				length_weight: Weight::zero(),
				class: DispatchClass::Operational,
				pays_fee: Pays::Yes,
			};
			let operational_limit =
				*<Test as Config>::BlockLength::get().max.get(DispatchClass::Operational) as usize;
			reset_check_weight(&op, normal_len_limit, false);
			reset_check_weight(&op, normal_len_limit + 100, false);
			reset_check_weight(&op, operational_limit, false);
			reset_check_weight(&op, operational_limit + 1, true);
		})
	}

	#[test]
	fn signed_ext_check_weight_works_normal_tx() {
		new_test_ext().execute_with(|| {
			let normal_limit = normal_weight_limit();
			let small =
				DispatchInfo { call_weight: Weight::from_parts(100, 0), ..Default::default() };
			let base_extrinsic = block_weights().get(DispatchClass::Normal).base_extrinsic;
			let medium =
				DispatchInfo { call_weight: normal_limit - base_extrinsic, ..Default::default() };
			let big = DispatchInfo {
				call_weight: normal_limit - base_extrinsic + Weight::from_parts(1, 0),
				..Default::default()
			};
			let len = 0_usize;

			let reset_check_weight = |i, f, s| {
				BlockWeight::<Test>::mutate(|current_weight| {
					current_weight.set(s, DispatchClass::Normal)
				});
				let r = CheckWeight::<Test>(PhantomData).validate_and_prepare(
					Some(1).into(),
					CALL,
					i,
					len,
					0,
				);
				if f {
					assert!(r.is_err())
				} else {
					assert!(r.is_ok())
				}
			};

			reset_check_weight(&small, false, Weight::zero());
			reset_check_weight(&medium, false, Weight::zero());
			reset_check_weight(&big, true, Weight::from_parts(1, 0));
		})
	}

	#[test]
	fn signed_ext_check_weight_refund_works() {
		new_test_ext().execute_with(|| {
			// This is half of the max block weight
			let info =
				DispatchInfo { call_weight: Weight::from_parts(512, 0), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::from_parts(128, 0)),
				pays_fee: Default::default(),
			};
			let len = 0_usize;
			let base_extrinsic = block_weights().get(DispatchClass::Normal).base_extrinsic;

			// We allow 75% for normal transaction, so we put 25% - extrinsic base weight
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::zero(), DispatchClass::Mandatory);
				current_weight
					.set(Weight::from_parts(256, 0) - base_extrinsic, DispatchClass::Normal);
			});

			let pre = CheckWeight::<Test>(PhantomData)
				.validate_and_prepare(Some(1).into(), CALL, &info, len, 0)
				.unwrap()
				.0;
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				info.total_weight() + Weight::from_parts(256, 0)
			);

			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(())
			));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				post_info.actual_weight.unwrap() + Weight::from_parts(256, 0)
			);
		})
	}

	#[test]
	fn signed_ext_check_weight_actual_weight_higher_than_max_is_capped() {
		new_test_ext().execute_with(|| {
			let info =
				DispatchInfo { call_weight: Weight::from_parts(512, 0), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::from_parts(700, 0)),
				pays_fee: Default::default(),
			};
			let len = 0_usize;

			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::zero(), DispatchClass::Mandatory);
				current_weight.set(Weight::from_parts(128, 0), DispatchClass::Normal);
			});

			let pre = CheckWeight::<Test>(PhantomData)
				.validate_and_prepare(Some(1).into(), CALL, &info, len, 0)
				.unwrap()
				.0;
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				info.total_weight() +
					Weight::from_parts(128, 0) +
					block_weights().get(DispatchClass::Normal).base_extrinsic,
			);

			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(())
			));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				info.total_weight() +
					Weight::from_parts(128, 0) +
					block_weights().get(DispatchClass::Normal).base_extrinsic,
			);
		})
	}

	#[test]
	fn extrinsic_already_refunded_more_precisely() {
		new_test_ext().execute_with(|| {
			// This is half of the max block weight
			let info =
				DispatchInfo { call_weight: Weight::from_parts(512, 0), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::from_parts(128, 0)),
				pays_fee: Default::default(),
			};
			let prior_block_weight = Weight::from_parts(64, 0);
			let accurate_refund = Weight::from_parts(510, 0);
			let len = 0_usize;
			let base_extrinsic = block_weights().get(DispatchClass::Normal).base_extrinsic;

			// Set initial info
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::zero(), DispatchClass::Mandatory);
				current_weight.set(prior_block_weight, DispatchClass::Normal);
			});

			// Validate and prepare extrinsic
			let pre = CheckWeight::<Test>(PhantomData)
				.validate_and_prepare(Some(1).into(), CALL, &info, len, 0)
				.unwrap()
				.0;

			assert_eq!(
				BlockWeight::<Test>::get().total(),
				info.total_weight() + prior_block_weight + base_extrinsic
			);

			// Refund more accurately than the benchmark
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.reduce(accurate_refund, DispatchClass::Normal);
			});
			crate::ExtrinsicWeightReclaimed::<Test>::put(accurate_refund);

			// Do the post dispatch
			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(())
			));

			// Ensure the accurate refund is used
			assert_eq!(crate::ExtrinsicWeightReclaimed::<Test>::get(), accurate_refund);
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				info.total_weight() - accurate_refund + prior_block_weight + base_extrinsic
			);
		})
	}

	#[test]
	fn extrinsic_already_refunded_less_precisely() {
		new_test_ext().execute_with(|| {
			// This is half of the max block weight
			let info =
				DispatchInfo { call_weight: Weight::from_parts(512, 0), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::from_parts(128, 0)),
				pays_fee: Default::default(),
			};
			let prior_block_weight = Weight::from_parts(64, 0);
			let inaccurate_refund = Weight::from_parts(110, 0);
			let len = 0_usize;
			let base_extrinsic = block_weights().get(DispatchClass::Normal).base_extrinsic;

			// Set initial info
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::zero(), DispatchClass::Mandatory);
				current_weight.set(prior_block_weight, DispatchClass::Normal);
			});

			// Validate and prepare extrinsic
			let pre = CheckWeight::<Test>(PhantomData)
				.validate_and_prepare(Some(1).into(), CALL, &info, len, 0)
				.unwrap()
				.0;

			let expected = info.total_weight() + prior_block_weight + base_extrinsic;
			assert_eq!(expected, BlockWeight::<Test>::get().total());
			assert_eq!(
				RuntimeBlockWeights::get().max_block - expected,
				System::remaining_block_weight().remaining()
			);

			// Refund less accurately than the benchmark
			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.reduce(inaccurate_refund, DispatchClass::Normal);
			});
			crate::ExtrinsicWeightReclaimed::<Test>::put(inaccurate_refund);

			// Do the post dispatch
			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(())
			));

			// Ensure the accurate refund from benchmark is used
			assert_eq!(
				crate::ExtrinsicWeightReclaimed::<Test>::get(),
				post_info.calc_unspent(&info)
			);
			let expected = post_info.actual_weight.unwrap() + prior_block_weight + base_extrinsic;
			assert_eq!(expected, BlockWeight::<Test>::get().total());
			assert_eq!(
				RuntimeBlockWeights::get().max_block - expected,
				System::remaining_block_weight().remaining()
			);
		})
	}

	#[test]
	fn zero_weight_extrinsic_still_has_base_weight() {
		new_test_ext().execute_with(|| {
			let weights = block_weights();
			let free = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
			let len = 0_usize;

			// Initial weight from `weights.base_block`
			assert_eq!(System::block_weight().total(), weights.base_block);
			assert_ok!(CheckWeight::<Test>(PhantomData).validate_and_prepare(
				Some(1).into(),
				CALL,
				&free,
				len,
				0,
			));
			assert_eq!(
				System::block_weight().total(),
				weights.get(DispatchClass::Normal).base_extrinsic + weights.base_block
			);
		})
	}

	#[test]
	fn normal_and_mandatory_tracked_separately() {
		new_test_ext().execute_with(|| {
			// Max block is 1024
			// Max normal is 768 (75%)
			// Max mandatory is unlimited
			let max_normal =
				DispatchInfo { call_weight: Weight::from_parts(753, 0), ..Default::default() };
			let mandatory = DispatchInfo {
				call_weight: Weight::from_parts(1019, 0),
				class: DispatchClass::Mandatory,
				..Default::default()
			};

			let len = 0_usize;

			let next_len = CheckWeight::<Test>::check_block_length(&max_normal, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&max_normal, next_len));
			assert_eq!(System::block_weight().total(), Weight::from_parts(768, 0));
			let next_len = CheckWeight::<Test>::check_block_length(&mandatory, len).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&mandatory, next_len));
			assert_eq!(block_weight_limit(), Weight::from_parts(1024, u64::MAX));
			assert_eq!(System::block_weight().total(), Weight::from_parts(1024 + 768, 0));
			assert_eq!(CheckWeight::<Test>::check_extrinsic_weight(&mandatory), Ok(()));
		});
	}

	#[test]
	fn no_max_total_should_still_be_limited_by_max_block() {
		// given
		let maximum_weight = BlockWeights::builder()
			.base_block(Weight::zero())
			.for_class(DispatchClass::non_mandatory(), |w| {
				w.base_extrinsic = Weight::zero();
				w.max_total = Some(Weight::from_parts(20, u64::MAX));
			})
			.for_class(DispatchClass::Mandatory, |w| {
				w.base_extrinsic = Weight::zero();
				w.reserved = Some(Weight::from_parts(5, u64::MAX));
				w.max_total = None;
			})
			.build_or_panic();
		let all_weight = crate::ConsumedWeight::new(|class| match class {
			DispatchClass::Normal => Weight::from_parts(10, 0),
			DispatchClass::Operational => Weight::from_parts(10, 0),
			DispatchClass::Mandatory => Weight::zero(),
		});
		assert_eq!(maximum_weight.max_block, all_weight.total().set_proof_size(u64::MAX));

		// fits into reserved
		let mandatory1 = DispatchInfo {
			call_weight: Weight::from_parts(5, 0),
			class: DispatchClass::Mandatory,
			..Default::default()
		};
		// does not fit into reserved and the block is full.
		let mandatory2 = DispatchInfo {
			call_weight: Weight::from_parts(6, 0),
			class: DispatchClass::Mandatory,
			..Default::default()
		};

		// when
		assert_ok!(calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&mandatory1,
		));
		assert_err!(
			calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
				&maximum_weight,
				all_weight,
				&mandatory2,
			),
			InvalidTransaction::ExhaustsResources
		);
	}

	#[test]
	fn check_extrinsic_proof_weight_includes_length() {
		new_test_ext().execute_with(|| {
			let with_len = |base_call_weight: Weight, len: usize| DispatchInfo {
				call_weight: base_call_weight,
				length_weight: Weight::from_parts(0, len as u64),
				class: DispatchClass::Normal,
				..Default::default()
			};

			let weights = block_weights();
			let max_extrinsic = weights.get(DispatchClass::Normal).max_extrinsic.unwrap();
			let max_proof_size = max_extrinsic.proof_size() as usize;

			// Base call weight that uses the full ref_time budget but zero proof_size.
			let base_weight = max_extrinsic.set_proof_size(0);

			// No length — should succeed.
			assert_ok!(CheckWeight::<Test>::check_extrinsic_weight(&with_len(base_weight, 0)));

			// Small length — should succeed.
			assert_ok!(CheckWeight::<Test>::check_extrinsic_weight(&with_len(base_weight, 100)));

			// Length exactly at the proof_size limit — should succeed.
			assert_ok!(CheckWeight::<Test>::check_extrinsic_weight(&with_len(
				base_weight,
				max_proof_size
			)));

			// One byte above the proof_size limit — should fail.
			assert_err!(
				CheckWeight::<Test>::check_extrinsic_weight(&with_len(
					base_weight,
					max_proof_size + 1
				)),
				InvalidTransaction::ExhaustsResources
			);

			// Call weight already at the full limit; adding any length should fail.
			let at_limit = with_len(max_extrinsic, 0);
			assert_ok!(CheckWeight::<Test>::check_extrinsic_weight(&at_limit));
			assert_err!(
				CheckWeight::<Test>::check_extrinsic_weight(&with_len(max_extrinsic, 1)),
				InvalidTransaction::ExhaustsResources
			);

			// Saturating: usize::MAX as proof_size saturates to u64::MAX which exceeds limits.
			assert_err!(
				CheckWeight::<Test>::check_extrinsic_weight(&with_len(Weight::zero(), usize::MAX)),
				InvalidTransaction::ExhaustsResources
			);
			assert_err!(
				CheckWeight::<Test>::check_extrinsic_weight(&with_len(
					Weight::from_parts(0, 10),
					usize::MAX
				)),
				InvalidTransaction::ExhaustsResources
			);
		});
	}

	#[test]
	fn proof_size_includes_length() {
		let maximum_weight = BlockWeights::builder()
			.base_block(Weight::zero())
			.for_class(DispatchClass::non_mandatory(), |w| {
				w.base_extrinsic = Weight::zero();
				w.max_total = Some(Weight::from_parts(20, 1000));
			})
			.for_class(DispatchClass::Mandatory, |w| {
				w.base_extrinsic = Weight::zero();
				w.max_total = Some(Weight::from_parts(20, 1000));
			})
			.build_or_panic();
		let all_weight = crate::ConsumedWeight::new(|class| match class {
			DispatchClass::Normal => Weight::from_parts(5, 0),
			DispatchClass::Operational => Weight::from_parts(5, 0),
			DispatchClass::Mandatory => Weight::from_parts(0, 0),
		});

		// The executive records the extrinsic length in `DispatchInfo::length_weight` before
		// calling into the extension pipeline. Tests must replicate this to get realistic
		// dispatch infos.
		let normal_base = DispatchInfo {
			call_weight: Weight::from_parts(5, 0),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let mandatory_base = DispatchInfo {
			call_weight: Weight::from_parts(5, 0),
			class: DispatchClass::Mandatory,
			..Default::default()
		};
		let with_len = |base: &DispatchInfo, len: u64| DispatchInfo {
			length_weight: Weight::from_parts(0, len),
			..*base
		};

		// Zero-length extrinsics — no proof_size contribution from len.
		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&normal_base,
		)
		.unwrap();
		assert_eq!(consumed.total().saturating_sub(all_weight.total()), normal_base.total_weight());

		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&mandatory_base,
		)
		.unwrap();
		assert_eq!(
			consumed.total().saturating_sub(all_weight.total()),
			mandatory_base.total_weight()
		);

		// Non-zero length: executive records len in `length_weight` before this call.
		let normal_100 = with_len(&normal_base, 100);
		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&normal_100,
		)
		.unwrap();
		// The proof_size in normal_100.total_weight() already includes the 100-byte contribution.
		assert_eq!(consumed.total().saturating_sub(all_weight.total()), normal_100.total_weight());

		let mandatory_100 = with_len(&mandatory_base, 100);
		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&mandatory_100,
		)
		.unwrap();
		// Must account for the len in the proof size
		assert_eq!(
			consumed.total().saturating_sub(all_weight.total()),
			mandatory_100.total_weight()
		);

		// Oversized: extrinsic length pushes proof_size over the per-class limit — errors out.
		let normal_oversized = with_len(&normal_base, 2000);
		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&normal_oversized,
		);
		// errors out
		assert_eq!(consumed, Err(InvalidTransaction::ExhaustsResources.into()));

		let mandatory_oversized = with_len(&mandatory_base, 2000);
		let consumed = calculate_consumed_weight::<<Test as Config>::RuntimeCall>(
			&maximum_weight,
			all_weight.clone(),
			&mandatory_oversized,
		);
		// errors out
		assert_eq!(consumed, Err(InvalidTransaction::ExhaustsResources.into()));
	}
}
