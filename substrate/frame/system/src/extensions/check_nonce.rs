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

use crate::{AccountInfo, Config};
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchInfo;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, One, TransactionExtension,
		TransactionExtensionBase, Zero,
	},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionValidityError, ValidTransaction,
	},
};
use sp_std::vec;

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

impl<T: Config> sp_std::fmt::Debug for CheckNonce<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "CheckNonce({})", self.0)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for CheckNonce<T> {
	const IDENTIFIER: &'static str = "CheckNonce";
	type Implicit = ();
	fn weight(&self) -> sp_weights::Weight {
		use super::WeightInfo;
		T::SystemExtensionsWeightInfo::check_nonce()
	}
}
impl<T: Config + Send + Sync, Context> TransactionExtension<T::RuntimeCall, Context>
	for CheckNonce<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
{
	type Val = (T::AccountId, AccountInfo<T::Nonce, T::AccountData>);
	type Pre = ();

	fn validate(
		&self,
		origin: <T as Config>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_context: &mut Context,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> Result<
		(sp_runtime::transaction_validity::ValidTransaction, Self::Val, T::RuntimeOrigin),
		sp_runtime::transaction_validity::TransactionValidityError,
	> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
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
			vec![Encode::encode(&(who.clone(), self.0 - One::one()))]
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
		Ok((validity, (who.clone(), account), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		let (who, mut account) = val;
		// `self.0 < account.nonce` already checked in `validate`.
		if self.0 > account.nonce {
			return Err(InvalidTransaction::Future.into())
		}
		account.nonce += T::Nonce::one();
		crate::Account::<T>::insert(who, account);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::assert_ok;
	use sp_runtime::traits::DispatchTransaction;

	#[test]
	fn signed_ext_check_nonce_works() {
		new_test_ext().execute_with(|| {
			crate::Account::<Test>::insert(
				1,
				crate::AccountInfo {
					nonce: 1,
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
				CheckNonce::<Test>(0)
					.validate_only(Some(1).into(), CALL, &info, len,)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Stale)
			);
			assert_eq!(
				CheckNonce::<Test>(0)
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				InvalidTransaction::Stale.into()
			);
			// correct
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(1).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).validate_and_prepare(
				Some(1).into(),
				CALL,
				&info,
				len
			));
			// future
			assert_ok!(CheckNonce::<Test>(5).validate_only(Some(1).into(), CALL, &info, len));
			assert_eq!(
				CheckNonce::<Test>(5)
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				InvalidTransaction::Future.into()
			);
		})
	}

	#[test]
	fn signed_ext_check_nonce_requires_provider() {
		new_test_ext().execute_with(|| {
			crate::Account::<Test>::insert(
				2,
				crate::AccountInfo {
					nonce: 1,
					consumers: 0,
					providers: 1,
					sufficients: 0,
					data: 0,
				},
			);
			crate::Account::<Test>::insert(
				3,
				crate::AccountInfo {
					nonce: 1,
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
				CheckNonce::<Test>(1)
					.validate_only(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Payment)
			);
			assert_eq!(
				CheckNonce::<Test>(1)
					.validate_and_prepare(Some(1).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::Payment)
			);
			// Non-zero providers
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(2).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).validate_and_prepare(
				Some(2).into(),
				CALL,
				&info,
				len
			));
			// Non-zero sufficients
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(3).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).validate_and_prepare(
				Some(3).into(),
				CALL,
				&info,
				len
			));
		})
	}
}
