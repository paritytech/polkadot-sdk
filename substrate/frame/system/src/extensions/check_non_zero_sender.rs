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
use codec::{Decode, Encode};
use core::marker::PhantomData;
<<<<<<< HEAD
use frame_support::{dispatch::DispatchInfo, DefaultNoBound};
=======
use frame_support::{pallet_prelude::TransactionSource, traits::OriginTrait, DefaultNoBound};
>>>>>>> 8e3d9296 ([Tx ext stage 2: 1/4] Add `TransactionSource` as argument in `TransactionExtension::validate` (#6323))
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, SignedExtension},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
};

/// Check to ensure that the sender is not the zero address.
#[derive(Encode, Decode, DefaultNoBound, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckNonZeroSender<T>(PhantomData<T>);

impl<T: Config + Send + Sync> core::fmt::Debug for CheckNonZeroSender<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "CheckNonZeroSender")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> CheckNonZeroSender<T> {
	/// Create new `SignedExtension` to check runtime version.
	pub fn new() -> Self {
		Self(core::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync> SignedExtension for CheckNonZeroSender<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type AccountId = T::AccountId;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();
	const IDENTIFIER: &'static str = "CheckNonZeroSender";

	fn additional_signed(&self) -> core::result::Result<(), TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.validate(who, call, info, len).map(|_| ())
	}

	fn validate(
		&self,
		who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
<<<<<<< HEAD
	) -> TransactionValidity {
		if who.using_encoded(|d| d.iter().all(|x| *x == 0)) {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::BadSigner))
=======
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> sp_runtime::traits::ValidateResult<Self::Val, T::RuntimeCall> {
		if let Some(who) = origin.as_signer() {
			if who.using_encoded(|d| d.iter().all(|x| *x == 0)) {
				return Err(InvalidTransaction::BadSigner.into())
			}
>>>>>>> 8e3d9296 ([Tx ext stage 2: 1/4] Add `TransactionSource` as argument in `TransactionExtension::validate` (#6323))
		}
		Ok(ValidTransaction::default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
<<<<<<< HEAD
	use frame_support::{assert_noop, assert_ok};
=======
	use frame_support::{assert_ok, dispatch::DispatchInfo};
	use sp_runtime::{
		traits::{AsTransactionAuthorizedOrigin, DispatchTransaction},
		transaction_validity::{TransactionSource::External, TransactionValidityError},
	};
>>>>>>> 8e3d9296 ([Tx ext stage 2: 1/4] Add `TransactionSource` as argument in `TransactionExtension::validate` (#6323))

	#[test]
	fn zero_account_ban_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
<<<<<<< HEAD
			assert_noop!(
				CheckNonZeroSender::<Test>::new().validate(&0, CALL, &info, len),
				InvalidTransaction::BadSigner
			);
			assert_ok!(CheckNonZeroSender::<Test>::new().validate(&1, CALL, &info, len));
=======
			assert_eq!(
				CheckNonZeroSender::<Test>::new()
					.validate_only(Some(0).into(), CALL, &info, len, External)
					.unwrap_err(),
				TransactionValidityError::from(InvalidTransaction::BadSigner)
			);
			assert_ok!(CheckNonZeroSender::<Test>::new().validate_only(
				Some(1).into(),
				CALL,
				&info,
				len,
				External,
			));
		})
	}

	#[test]
	fn unsigned_origin_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			let (_, _, origin) = CheckNonZeroSender::<Test>::new()
				.validate(None.into(), CALL, &info, len, (), CALL, External)
				.unwrap();
			assert!(!origin.is_transaction_authorized());
>>>>>>> 8e3d9296 ([Tx ext stage 2: 1/4] Add `TransactionSource` as argument in `TransactionExtension::validate` (#6323))
		})
	}
}
