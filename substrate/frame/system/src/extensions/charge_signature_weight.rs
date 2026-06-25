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

//! Charges signature verification weight for signed extrinsics.
//!
//! Signature verification itself happens while converting an [`UncheckedExtrinsic`] into a
//! [`CheckedExtrinsic`]. When [`ExtrinsicBaseWeight`] is benchmarked without signature cost, this
//! extension re-adds that weight for signed transactions.
//!
//! [`UncheckedExtrinsic`]: sp_runtime::generic::UncheckedExtrinsic
//! [`CheckedExtrinsic`]: sp_runtime::generic::CheckedExtrinsic
//! [`ExtrinsicBaseWeight`]: frame_support::weights::constants::ExtrinsicBaseWeight

use crate::Config;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{pallet_prelude::TransactionSource, traits::Get};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{DispatchInfoOf, TransactionExtension},
};
use sp_weights::Weight;

/// Zero [`sp_weights::Weight`] provider for default configurations.
pub struct ZeroWeight;

impl Get<Weight> for ZeroWeight {
	fn get() -> Weight {
		Weight::zero()
	}
}

/// Charges [`Config::SignatureWeight`] when the extrinsic is signed.
///
/// Include this extension in the runtime transaction-extension tuple. Construct it with
/// [`Self::signed`] for signed extrinsics and [`Self::unsigned`] for general or unsigned paths.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ChargeSignatureWeight<T> {
	is_signed: bool,
	_phantom: PhantomData<T>,
}

impl<T> Default for ChargeSignatureWeight<T> {
	fn default() -> Self {
		Self { is_signed: false, _phantom: PhantomData }
	}
}

impl<T: Config + Send + Sync> core::fmt::Debug for ChargeSignatureWeight<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "ChargeSignatureWeight(signed: {})", self.is_signed)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> ChargeSignatureWeight<T> {
	/// Create an extension that charges signature verification weight.
	pub fn signed() -> Self {
		Self { is_signed: true, _phantom: PhantomData }
	}

	/// Create an extension that does not charge signature verification weight.
	pub fn unsigned() -> Self {
		Self { is_signed: false, _phantom: PhantomData }
	}
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for ChargeSignatureWeight<T> {
	const IDENTIFIER: &'static str = "ChargeSignatureWeight";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn weight(&self, _: &T::RuntimeCall) -> Weight {
		if self.is_signed {
			T::SignatureWeight::get()
		} else {
			Weight::zero()
		}
	}

	fn validate(
		&self,
		origin: <T as Config>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> sp_runtime::traits::ValidateResult<Self::Val, T::RuntimeCall> {
		Ok((Default::default(), (), origin))
	}

	impl_tx_ext_default!(T::RuntimeCall; prepare);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, CALL};
	use frame_support::{assert_ok, dispatch::DispatchInfo};
	use sp_runtime::{
		traits::{AsTransactionAuthorizedOrigin, DispatchTransaction, TxBaseImplication},
		transaction_validity::TransactionSource::External,
	};

	#[test]
	fn signed_charges_configured_weight() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			assert_eq!(
				ChargeSignatureWeight::<Test>::signed().weight(CALL),
				<Test as Config>::SignatureWeight::get(),
			);
			assert_eq!(ChargeSignatureWeight::<Test>::unsigned().weight(CALL), Weight::zero());
			assert_ok!(ChargeSignatureWeight::<Test>::signed().validate_only(
				Some(1).into(),
				CALL,
				&info,
				len,
				External,
				0,
			));
		});
	}

	#[test]
	fn unsigned_origin_passes_through() {
		new_test_ext().execute_with(|| {
			let info = DispatchInfo::default();
			let len = 0_usize;
			let (_, _, origin) = ChargeSignatureWeight::<Test>::unsigned()
				.validate(None.into(), CALL, &info, len, (), &TxBaseImplication(CALL), External)
				.unwrap();
			assert!(!origin.is_transaction_authorized());
		});
	}
}
