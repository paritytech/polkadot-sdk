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
use frame_support::{dispatch::DispatchInfo, DefaultNoBound};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, SignedExtension, TransactionExtension},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
};
use sp_std::{marker::PhantomData, prelude::*};

/// Check to ensure that the sender is not the zero address.
#[derive(Encode, Decode, DefaultNoBound, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckNonZeroSender<T>(PhantomData<T>);

impl<T: Config + Send + Sync> sp_std::fmt::Debug for CheckNonZeroSender<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "CheckNonZeroSender")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> CheckNonZeroSender<T> {
	/// Create new `SignedExtension` to check runtime version.
	pub fn new() -> Self {
		Self(sp_std::marker::PhantomData)
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

	fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		SignedExtension::validate(&self, who, call, info, len).map(|_| ())
	}

	fn validate(
		&self,
		who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		if who.using_encoded(|d| d.iter().all(|x| *x == 0)) {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::BadSigner))
		}
		Ok(ValidTransaction::default())
	}
}

impl<T: Config + Send + Sync> TransactionExtension for CheckNonZeroSender<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	const IDENTIFIER: &'static str = "CheckNonZeroSender";
	type Call = T::RuntimeCall;
	type Pre = ();
	type Val = ();
	type Implicit = ();

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(())
	}

	fn prepare(
		self,
		_val: Self::Val,
		origin: &<Self::Call as Dispatchable>::RuntimeOrigin,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		TransactionExtension::validate(&self, origin.clone(), call, info, len, &[]).map(|_| ())
	}

	fn validate(
		&self,
		origin: <Self::Call as sp_runtime::traits::Dispatchable>::RuntimeOrigin,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
		_implicit: &[u8],
	) -> Result<
		(
			sp_runtime::transaction_validity::ValidTransaction,
			Self::Val,
			<Self::Call as sp_runtime::traits::Dispatchable>::RuntimeOrigin,
		),
		sp_runtime::transaction_validity::TransactionValidityError,
	> {
		let who =
			crate::ensure_signed(origin.clone()).map_err(|_| InvalidTransaction::BadSigner)?;
		if who.using_encoded(|d| d.iter().all(|x| *x == 0)) {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::BadSigner))
		}
		Ok((ValidTransaction::default(), (), origin))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::assert_ok;

	#[test]
	fn zero_account_ban_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			assert_eq!(
				TransactionExtension::validate(
					&CheckNonZeroSender::<Test>::new(),
					Some(0).into(),
					CALL,
					&info,
					len,
					&[]
				)
				.unwrap_err(),
				TransactionValidityError::Invalid(InvalidTransaction::BadSigner)
			);
			assert_ok!(TransactionExtension::validate(
				&CheckNonZeroSender::<Test>::new(),
				Some(1).into(),
				CALL,
				&info,
				len,
				&[]
			));
		})
	}
}
