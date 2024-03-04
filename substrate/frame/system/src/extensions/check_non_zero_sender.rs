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
use frame_support::{traits::OriginTrait, DefaultNoBound};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		transaction_extension::TransactionExtensionBase, DispatchInfoOf, TransactionExtension,
	},
	transaction_validity::InvalidTransaction,
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
	/// Create new `TransactionExtension` to check runtime version.
	pub fn new() -> Self {
		Self(sp_std::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for CheckNonZeroSender<T> {
	const IDENTIFIER: &'static str = "CheckNonZeroSender";
	type Implicit = ();
	fn weight(&self) -> sp_weights::Weight {
		<T::ExtensionsWeightInfo as super::WeightInfo>::check_non_zero_sender()
	}
}
impl<T: Config + Send + Sync, Context> TransactionExtension<T::RuntimeCall, Context>
	for CheckNonZeroSender<T>
{
	type Val = ();
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
	) -> sp_runtime::traits::ValidateResult<Self::Val, T::RuntimeCall> {
		if let Some(who) = origin.as_system_signer() {
			if who.using_encoded(|d| d.iter().all(|x| *x == 0)) {
				return Err(InvalidTransaction::BadSigner.into())
			}
		}
		Ok((Default::default(), (), origin))
	}
	impl_tx_ext_default!(T::RuntimeCall; Context; prepare);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::{assert_ok, dispatch::DispatchInfo};
	use sp_runtime::{traits::DispatchTransaction, TransactionValidityError};

	#[test]
	fn zero_account_ban_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			assert_eq!(
				CheckNonZeroSender::<Test>::new()
					.validate_only(Some(0).into(), CALL, &info, len)
					.unwrap_err(),
				TransactionValidityError::from(InvalidTransaction::BadSigner)
			);
			assert_ok!(CheckNonZeroSender::<Test>::new().validate_only(
				Some(1).into(),
				CALL,
				&info,
				len
			));
		})
	}

	#[test]
	fn unsigned_origin_works() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			assert_ok!(CheckNonZeroSender::<Test>::new().validate_only(
				None.into(),
				CALL,
				&info,
				len
			));
		})
	}
}
