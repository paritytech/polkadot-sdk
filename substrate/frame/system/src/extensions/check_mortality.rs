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

use crate::{pallet_prelude::BlockNumberFor, BlockHash, Config, Pallet};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	generic::Era,
	impl_tx_ext_default,
	traits::{
		DispatchInfoOf, SaturatedConversion, TransactionExtension, TransactionExtensionBase,
		ValidateResult,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

/// Check for transaction mortality.
///
/// # Transaction Validity
///
/// The extension affects `longevity` of the transaction according to the [`Era`] definition.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckMortality<T: Config + Send + Sync>(pub Era, sp_std::marker::PhantomData<T>);

impl<T: Config + Send + Sync> CheckMortality<T> {
	/// utility constructor. Used only in client/factory code.
	pub fn from(era: Era) -> Self {
		Self(era, sp_std::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync> sp_std::fmt::Debug for CheckMortality<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "CheckMortality({:?})", self.0)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for CheckMortality<T> {
	const IDENTIFIER: &'static str = "CheckMortality";
	type Implicit = T::Hash;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		let current_u64 = <Pallet<T>>::block_number().saturated_into::<u64>();
		let n = self.0.birth(current_u64).saturated_into::<BlockNumberFor<T>>();
		if !<BlockHash<T>>::contains_key(n) {
			Err(InvalidTransaction::AncientBirthBlock.into())
		} else {
			Ok(<Pallet<T>>::block_hash(n))
		}
	}
	fn weight(&self) -> sp_weights::Weight {
		<T::ExtensionsWeightInfo as super::WeightInfo>::check_mortality()
	}
}
impl<T: Config + Send + Sync, Context> TransactionExtension<T::RuntimeCall, Context>
	for CheckMortality<T>
{
	type Pre = ();
	type Val = ();

	fn validate(
		&self,
		origin: <T as Config>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_context: &mut Context,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let current_u64 = <Pallet<T>>::block_number().saturated_into::<u64>();
		let valid_till = self.0.death(current_u64);
		Ok((
			ValidTransaction {
				longevity: valid_till.saturating_sub(current_u64),
				..Default::default()
			},
			(),
			origin,
		))
	}
	impl_tx_ext_default!(T::RuntimeCall; Context; prepare);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, System, Test, CALL};
	use frame_support::{
		dispatch::{DispatchClass, DispatchInfo, Pays},
		weights::Weight,
	};
	use sp_core::H256;
	use sp_runtime::traits::DispatchTransaction;

	#[test]
	fn signed_ext_check_era_should_work() {
		new_test_ext().execute_with(|| {
			// future
			assert_eq!(
				CheckMortality::<Test>::from(Era::mortal(4, 2)).implicit().err().unwrap(),
				InvalidTransaction::AncientBirthBlock.into(),
			);

			// correct
			System::set_block_number(13);
			<BlockHash<Test>>::insert(12, H256::repeat_byte(1));
			assert!(CheckMortality::<Test>::from(Era::mortal(4, 12)).implicit().is_ok());
		})
	}

	#[test]
	fn signed_ext_check_era_should_change_longevity() {
		new_test_ext().execute_with(|| {
			let normal = DispatchInfo {
				weight: Weight::from_parts(100, 0),
				class: DispatchClass::Normal,
				pays_fee: Pays::Yes,
			};
			let len = 0_usize;
			let ext = (
				crate::CheckWeight::<Test>::new(),
				CheckMortality::<Test>::from(Era::mortal(16, 256)),
			);
			System::set_block_number(17);
			<BlockHash<Test>>::insert(16, H256::repeat_byte(1));

			assert_eq!(
				ext.validate_only(Some(1).into(), CALL, &normal, len).unwrap().0.longevity,
				15
			);
		})
	}
}
