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
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, One, TransactionExtension,
		ValidateResult, Zero,
	},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionValidityError, ValidTransaction,
	},
	Saturating,
};

/// Nonce check and increment to give replay protection for transactions.
///
/// # Transaction Validity
///
/// This extension affects `requires` and `provides` tags of validity, but DOES NOT
/// set the `priority` field. Make sure that AT LEAST one of the transaction extension sets
/// some kind of priority upon validating transactions.
///
/// The preparation step assumes that the nonce information has not changed since the validation
/// step. This means that other extensions ahead of `CheckNonce` in the pipeline must not alter the
/// nonce during their own preparation step, or else the transaction may be rejected during dispatch
/// or lead to an inconsistent account state.
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

impl<T: Config> TransactionExtension<T::RuntimeCall> for CheckNonce<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "CheckNonce";
	type Implicit = ();
	type Val = Option<(T::AccountId, T::Nonce)>;
	type Pre = ();

	fn weight(&self, _: &T::RuntimeCall) -> sp_weights::Weight {
		<T::ExtensionsWeightInfo as super::WeightInfo>::check_nonce()
	}

	fn validate(
		&self,
		origin: <T as Config>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let Some(who) = origin.as_system_origin_signer() else {
			return Ok((Default::default(), None, origin))
		};
		let account = crate::Account::<T>::get(who);
		if account.providers.is_zero() && account.sufficients.is_zero() {
			// Nonce storage not paid for
			return Err(InvalidTransaction::Payment.into())
		}
		if self.0 < account.nonce {
			return Err(InvalidTransaction::Stale.into())
		}

		let provides = vec![Encode::encode(&(who.clone(), self.0))];
		let requires = if account.nonce < self.0 {
			vec![Encode::encode(&(who.clone(), self.0.saturating_sub(One::one())))]
		} else {
			vec![]
		};

		let validity = ValidTransaction {
			priority: 0,
			requires,
			provides,
			longevity: TransactionLongevity::max_value(),
			propagate: true,
		};

		Ok((validity, Some((who.clone(), account.nonce)), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let Some((who, mut nonce)) = val else { return Ok(()) };
		// `self.0 < nonce` already checked in `validate`.
		if self.0 > nonce {
			return Err(InvalidTransaction::Future.into())
		}
		nonce += T::Nonce::one();
		crate::Account::<T>::mutate(who, |account| account.nonce = nonce);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::assert_ok;
	use sp_runtime::traits::{AsAuthorizedOrigin, DispatchTransaction};

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
			assert_eq!(
				CheckNonce::<Test>(0u64.into())
					.validate_only(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Stale)
			);
			assert_eq!(
				CheckNonce::<Test>(0u64.into())
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Stale)
			);
			// correct
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_only(
				Some(1).into(),
				CALL,
				&info,
				len
			));
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_and_prepare(
				Some(1).into(),
				CALL,
				&info,
				len
			));
			// future
			assert_ok!(CheckNonce::<Test>(5u64.into()).validate_only(
				Some(1).into(),
				CALL,
				&info,
				len
			));
			assert_eq!(
				CheckNonce::<Test>(5u64.into())
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Future)
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
			assert_eq!(
				CheckNonce::<Test>(1u64.into())
					.validate_only(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Payment)
			);
			assert_eq!(
				CheckNonce::<Test>(1u64.into())
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Payment)
			);
			// Non-zero providers
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_only(
				Some(2).into(),
				CALL,
				&info,
				len
			));
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_and_prepare(
				Some(2).into(),
				CALL,
				&info,
				len
			));
			// Non-zero sufficients
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_only(
				Some(3).into(),
				CALL,
				&info,
				len
			));
			assert_ok!(CheckNonce::<Test>(1u64.into()).validate_and_prepare(
				Some(3).into(),
				CALL,
				&info,
				len
			));
		})
	}

	#[test]
	fn unsigned_check_nonce_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			let (_, val, origin) = CheckNonce::<Test>(1u64.into())
				.validate(None.into(), CALL, &info, len, (), CALL)
				.unwrap();
			assert!(!origin.is_authorized());
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
				.validate(Some(1).into(), CALL, &info, len, (), CALL)
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
}
