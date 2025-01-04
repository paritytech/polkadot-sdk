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
use alloc::vec;
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchInfo;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, One, SignedExtension, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionValidity, TransactionValidityError,
		ValidTransaction,
	},
};

/// Nonce check and increment to give replay protection for transactions.
///
/// # Transaction Validity
///
/// This extension affects `requires` and `provides` tags of validity, but DOES NOT
/// set the `priority` field. Make sure that AT LEAST one of the signed extension sets
/// some kind of priority upon validating transactions.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckNonce<T: Config>(#[codec(compact)] pub T::Nonce);

impl<T: Config> CheckNonce<T> {
	/// utility constructor. Used only in client/factory code.
	pub fn from(nonce: T::Nonce) -> Self {
		Self(nonce)
	}
}

impl<T: Config> core::fmt::Debug for CheckNonce<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "CheckNonce({})", self.0)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T: Config> SignedExtension for CheckNonce<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type AccountId = T::AccountId;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();
	const IDENTIFIER: &'static str = "CheckNonce";

	fn additional_signed(&self) -> core::result::Result<(), TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<(), TransactionValidityError> {
		let mut account = crate::Account::<T>::get(who);
		if account.providers.is_zero() && account.sufficients.is_zero() {
			// Nonce storage not paid for
			return Err(InvalidTransaction::Payment.into())
		}
		if self.0 != account.nonce {
			return Err(if self.0 < account.nonce {
				InvalidTransaction::Stale
			} else {
				InvalidTransaction::Future
			}
			.into())
		}
		account.nonce += T::Nonce::one();
		crate::Account::<T>::insert(who, account);
		Ok(())
	}

	fn validate(
		&self,
		who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		let account = crate::Account::<T>::get(who);
		if account.providers.is_zero() && account.sufficients.is_zero() {
			// Nonce storage not paid for
			return InvalidTransaction::Payment.into()
		}
		if self.0 < account.nonce {
			return InvalidTransaction::Stale.into()
		}

		let provides = vec![Encode::encode(&(who, self.0))];
		let requires = if account.nonce < self.0 {
			vec![Encode::encode(&(who, self.0 - One::one()))]
		} else {
			vec![]
		};

		Ok(ValidTransaction {
			priority: 0,
			requires,
			provides,
			longevity: TransactionLongevity::max_value(),
			propagate: true,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
<<<<<<< HEAD
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::{assert_noop, assert_ok};
=======
	use crate::mock::{new_test_ext, RuntimeCall, Test, CALL};
	use frame_support::{
		assert_ok, assert_storage_noop, dispatch::GetDispatchInfo, traits::OriginTrait,
	};
	use sp_runtime::{
		traits::{AsTransactionAuthorizedOrigin, DispatchTransaction, TxBaseImplication},
		transaction_validity::TransactionSource::External,
	};
>>>>>>> b5a5ac4 (Make `TransactionExtension` tuple of tuple transparent for implication (#7028))

	#[test]
	fn signed_ext_check_nonce_works() {
		new_test_ext().execute_with(|| {
			crate::Account::<Test>::insert(
				1,
				crate::AccountInfo {
					nonce: 1u64.into(),
					consumers: 0,
					providers: 1,
					sufficients: 0,
					data: 0,
				},
			);
			let info = DispatchInfo::default();
			let len = 0_usize;
			// stale
			assert_noop!(
				CheckNonce::<Test>(0u64.into()).validate(&1, CALL, &info, len),
				InvalidTransaction::Stale
			);
			assert_noop!(
				CheckNonce::<Test>(0u64.into()).pre_dispatch(&1, CALL, &info, len),
				InvalidTransaction::Stale
			);
			// correct
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate(&1, CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1u64.into()).pre_dispatch(&1, CALL, &info, len));
			// future
			assert_ok!(CheckNonce::<Test>(5u64.into()).validate(&1, CALL, &info, len));
			assert_noop!(
				CheckNonce::<Test>(5u64.into()).pre_dispatch(&1, CALL, &info, len),
				InvalidTransaction::Future
			);
		})
	}

	#[test]
	fn signed_ext_check_nonce_requires_provider() {
		new_test_ext().execute_with(|| {
			crate::Account::<Test>::insert(
				2,
				crate::AccountInfo {
					nonce: 1u64.into(),
					consumers: 0,
					providers: 1,
					sufficients: 0,
					data: 0,
				},
			);
			crate::Account::<Test>::insert(
				3,
				crate::AccountInfo {
					nonce: 1u64.into(),
					consumers: 0,
					providers: 0,
					sufficients: 1,
					data: 0,
				},
			);
			let info = DispatchInfo::default();
			let len = 0_usize;
			// Both providers and sufficients zero
<<<<<<< HEAD
			assert_noop!(
				CheckNonce::<Test>(1u64.into()).validate(&1, CALL, &info, len),
				InvalidTransaction::Payment
			);
			assert_noop!(
				CheckNonce::<Test>(1u64.into()).pre_dispatch(&1, CALL, &info, len),
				InvalidTransaction::Payment
			);
			// Non-zero providers
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate(&2, CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1u64.into()).pre_dispatch(&2, CALL, &info, len));
			// Non-zero sufficients
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate(&3, CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1u64.into()).pre_dispatch(&3, CALL, &info, len));
=======
			assert_storage_noop!({
				assert_eq!(
					CheckNonce::<Test>(1u64.into())
						.validate_only(Some(1).into(), CALL, &info, len, External, 0)
						.unwrap_err(),
					TransactionValidityError::Invalid(InvalidTransaction::Payment)
				);
				assert_eq!(
					CheckNonce::<Test>(1u64.into())
						.validate_and_prepare(Some(1).into(), CALL, &info, len, 0)
						.unwrap_err(),
					TransactionValidityError::Invalid(InvalidTransaction::Payment)
				);
			});
			// Non-zero providers
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_only(
				Some(2).into(),
				CALL,
				&info,
				len,
				External,
				0,
			));
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_and_prepare(
				Some(2).into(),
				CALL,
				&info,
				len,
				0,
			));
			// Non-zero sufficients
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_only(
				Some(3).into(),
				CALL,
				&info,
				len,
				External,
				0,
			));
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_and_prepare(
				Some(3).into(),
				CALL,
				&info,
				len,
				0,
			));
		})
	}

	#[test]
	fn unsigned_check_nonce_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			let (_, val, origin) = CheckNonce::<Test>(1u64.into())
				.validate(None.into(), CALL, &info, len, (), &TxBaseImplication(CALL), External)
				.unwrap();
			assert!(!origin.is_transaction_authorized());
			assert_ok!(CheckNonce::<Test>(1u64.into()).prepare(val, &origin, CALL, &info, len));
		})
	}

	#[test]
	fn check_nonce_preserves_account_data() {
		new_test_ext().execute_with(|| {
			crate::Account::<Test>::insert(
				1,
				crate::AccountInfo {
					nonce: 1u64.into(),
					consumers: 0,
					providers: 1,
					sufficients: 0,
					data: 0,
				},
			);
			let info = DispatchInfo::default();
			let len = 0_usize;
			// run the validation step
			let (_, val, origin) = CheckNonce::<Test>(1u64.into())
				.validate(Some(1).into(), CALL, &info, len, (), &TxBaseImplication(CALL), External)
				.unwrap();
			// mutate `AccountData` for the caller
			crate::Account::<Test>::mutate(1, |info| {
				info.data = 42;
			});
			// run the preparation step
			assert_ok!(CheckNonce::<Test>(1u64.into()).prepare(val, &origin, CALL, &info, len));
			// only the nonce should be altered by the preparation step
			let expected_info = crate::AccountInfo {
				nonce: 2u64.into(),
				consumers: 0,
				providers: 1,
				sufficients: 0,
				data: 42,
			};
			assert_eq!(crate::Account::<Test>::get(1), expected_info);
		})
	}

	#[test]
	fn check_nonce_skipped_and_refund_for_other_origins() {
		new_test_ext().execute_with(|| {
			let ext = CheckNonce::<Test>(1u64.into());

			let mut info = CALL.get_dispatch_info();
			info.extension_weight = ext.weight(CALL);

			// Ensure we test the refund.
			assert!(info.extension_weight != Weight::zero());

			let len = CALL.encoded_size();

			let origin = crate::RawOrigin::Root.into();
			let (pre, origin) = ext.validate_and_prepare(origin, CALL, &info, len, 0).unwrap();

			assert!(origin.as_system_ref().unwrap().is_root());

			let pd_res = Ok(());
			let mut post_info = frame_support::dispatch::PostDispatchInfo {
				actual_weight: Some(info.total_weight()),
				pays_fee: Default::default(),
			};

			<CheckNonce<Test> as TransactionExtension<RuntimeCall>>::post_dispatch(
				pre,
				&info,
				&mut post_info,
				len,
				&pd_res,
			)
			.unwrap();

			assert_eq!(post_info.actual_weight, Some(info.call_weight));
>>>>>>> b5a5ac4 (Make `TransactionExtension` tuple of tuple transparent for implication (#7028))
		})
	}
}
