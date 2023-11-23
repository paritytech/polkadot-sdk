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
use frame_support::dispatch::DispatchInfo;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{DispatchInfoOf, Dispatchable, One, SignedExtension, TransactionExtension, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionValidity, TransactionValidityError,
		ValidTransaction,
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

impl<T: Config> SignedExtension for CheckNonce<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type AccountId = T::AccountId;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();
	const IDENTIFIER: &'static str = "CheckNonce";

	fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
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

impl<T: Config> TransactionExtension<T::RuntimeCall> for CheckNonce<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	const IDENTIFIER: &'static str = "CheckNonce";
	type Pre = ();
	type Val = ();
	type Implicit = ();

	fn prepare(
		self,
		_val: Self::Val,
		origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let who =
			crate::ensure_signed(origin.clone()).map_err(|_| InvalidTransaction::BadSigner)?;
		let mut account = crate::Account::<T>::get(who.clone());
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
		origin: <T as Config>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> Result<
		(sp_runtime::transaction_validity::ValidTransaction, Self::Val, T::RuntimeOrigin),
		sp_runtime::transaction_validity::TransactionValidityError,
	> {
		let who =
			crate::ensure_signed(origin.clone()).map_err(|_| InvalidTransaction::BadSigner)?;
		let account = crate::Account::<T>::get(who.clone());
		if account.providers.is_zero() && account.sufficients.is_zero() {
			// Nonce storage not paid for
			return Err(InvalidTransaction::Payment.into())
		}
		if self.0 < account.nonce {
			return Err(InvalidTransaction::Stale.into())
		}

		let provides = vec![Encode::encode(&(who.clone(), self.0))];
		let requires = if account.nonce < self.0 {
			vec![Encode::encode(&(who, self.0 - One::one()))]
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
		Ok((validity, (), origin))
	}
	impl_tx_ext_default!(T::RuntimeCall; implicit);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::{assert_noop, assert_ok};
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
			assert_noop!(
				CheckNonce::<Test>(0).prepare((), &Some(1).into(), CALL, &info, len),
				InvalidTransaction::Stale
			);
			// correct
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(1).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).prepare((), &Some(1).into(), CALL, &info, len));
			// future
			assert_ok!(CheckNonce::<Test>(5).validate_only(Some(1).into(), CALL, &info, len));
			assert_noop!(
				CheckNonce::<Test>(5).prepare((), &Some(1).into(), CALL, &info, len),
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
			assert_noop!(
				CheckNonce::<Test>(1).prepare((), &Some(1).into(), CALL, &info, len),
				InvalidTransaction::Payment
			);
			// Non-zero providers
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(2).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).prepare((), &Some(2).into(), CALL, &info, len));
			// Non-zero sufficients
			assert_ok!(CheckNonce::<Test>(1).validate_only(Some(3).into(), CALL, &info, len));
			assert_ok!(CheckNonce::<Test>(1).prepare((), &Some(3).into(), CALL, &info, len));
		})
	}
}
