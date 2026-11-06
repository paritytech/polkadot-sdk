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

//! Contains transaction extensions needed for ethereum compatability.

use crate::{CallOf, Config, Origin, OriginFor, SubstrateTxSigner, WeightInfo};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	DebugNoBound, DefaultNoBound,
	pallet_prelude::{InvalidTransaction, TransactionSource},
	traits::OriginTrait,
};
use scale_info::TypeInfo;
use sp_runtime::{
	DispatchResult, Weight,
	traits::{DispatchInfoOf, PostDispatchInfoOf, TransactionExtension, ValidateResult},
	transaction_validity::TransactionValidityError,
};

/// An extension that sets the origin to [`Origin::EthTransaction`] in case it originated from an
/// eth transaction.
///
/// For a signed substrate transaction it records the signer in `SubstrateTxSigner` for the
/// duration of the dispatch. The pallet needs it to tell whether a signed origin had its nonce
/// consumed by `CheckNonce`.
///
/// This extension needs to be put behind any other extension that relies on a signed origin.
/// Every extension pipeline that dispatches signed substrate calls into pallet-revive needs it,
/// including the one of meta transactions. Without it the signer's first deploy is not given
/// the nonce `CheckNonce` consumed, so its CREATE1 address is not the one dry runs predict.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Eq,
	PartialEq,
	DefaultNoBound,
	TypeInfo,
	DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct SetOrigin<T: Config + Send + Sync> {
	/// Skipped as can only be set by runtime code.
	#[codec(skip)]
	is_eth_transaction: bool,
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config + Send + Sync> SetOrigin<T> {
	/// Create the extension so that it will transform the origin.
	///
	/// If the extension is default constructed it records the signer of a substrate transaction
	/// instead.
	pub fn new_from_eth_transaction() -> Self {
		Self { is_eth_transaction: true, _phantom: Default::default() }
	}
}

impl<T> TransactionExtension<CallOf<T>> for SetOrigin<T>
where
	T: Config + Send + Sync,
	OriginFor<T>: From<Origin<T>>,
{
	const IDENTIFIER: &'static str = "EthSetOrigin";
	type Implicit = ();
	type Pre = SetOriginPre<T::AccountId>;
	type Val = ();

	fn weight(&self, _: &CallOf<T>) -> Weight {
		if self.is_eth_transaction {
			Weight::zero()
		} else {
			T::WeightInfo::set_origin_substrate_tx()
		}
	}

	fn validate(
		&self,
		origin: OriginFor<T>,
		_call: &CallOf<T>,
		_info: &DispatchInfoOf<CallOf<T>>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, CallOf<T>> {
		let origin = if self.is_eth_transaction {
			let signer =
				frame_system::ensure_signed(origin).map_err(|_| InvalidTransaction::BadProof)?;
			Origin::EthTransaction(signer).into()
		} else {
			origin
		};
		Ok((Default::default(), Default::default(), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		origin: &OriginFor<T>,
		_call: &CallOf<T>,
		_info: &DispatchInfoOf<CallOf<T>>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		if self.is_eth_transaction {
			return Ok(SetOriginPre::EthTransaction);
		}
		let Some(signer) = origin.as_signer() else { return Ok(SetOriginPre::NotSigned) };
		let previous = SubstrateTxSigner::<T>::mutate(|record| record.replace(signer.clone()));
		// When a meta transaction is relayed by its own signer, `CheckNonce` ran twice for the
		// same account and the outer record would point at the nonce the inner one consumed.
		Ok(SetOriginPre::Recorded(previous.filter(|previous| previous != signer)))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfoOf<CallOf<T>>,
		_post_info: &PostDispatchInfoOf<CallOf<T>>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		match pre {
			SetOriginPre::EthTransaction => Ok(Weight::zero()),
			SetOriginPre::Recorded(previous) => {
				SubstrateTxSigner::<T>::set(previous);
				Ok(Weight::zero())
			},
			SetOriginPre::NotSigned => Ok(T::WeightInfo::set_origin_substrate_tx()),
		}
	}
}

/// What [`SetOrigin`] did before dispatch, so that it can be undone afterwards.
pub enum SetOriginPre<AccountId> {
	/// Nothing was charged or recorded.
	EthTransaction,
	/// The signer was recorded over the record of an enclosing transaction, which is restored
	/// after dispatch. This happens for meta transactions.
	Recorded(Option<AccountId>),
	/// The origin is not signed, so the weight charged for recording it is refunded.
	NotSigned,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		test_utils::{ALICE, BOB},
		tests::{ExtBuilder, RuntimeCall, RuntimeOrigin, Test},
	};
	use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
	use sp_runtime::{DispatchResultWithInfo, traits::DispatchTransaction};

	const ACTUAL_WEIGHT: Weight = Weight::from_parts(1_000_000_000, 0);

	fn run(
		ext: SetOrigin<Test>,
		origin: RuntimeOrigin,
		dispatch: impl FnOnce() -> DispatchResultWithInfo<PostDispatchInfo>,
	) -> Weight {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		ext.test_run(origin, &call, &call.get_dispatch_info(), 0, 0, |_| dispatch())
			.unwrap()
			.unwrap()
			.actual_weight
			.unwrap()
	}

	fn dispatched() -> DispatchResultWithInfo<PostDispatchInfo> {
		Ok(PostDispatchInfo { actual_weight: Some(ACTUAL_WEIGHT), pays_fee: Default::default() })
	}

	#[test]
	fn refunds_recording_weight_only_when_charged_and_unused() {
		ExtBuilder::default().build().execute_with(|| {
			let eth = SetOrigin::<Test>::new_from_eth_transaction();
			assert_eq!(run(eth, RuntimeOrigin::signed(ALICE), dispatched), ACTUAL_WEIGHT);
			assert_eq!(
				run(SetOrigin::default(), RuntimeOrigin::signed(ALICE), dispatched),
				ACTUAL_WEIGHT,
			);
			assert_eq!(
				run(SetOrigin::default(), RuntimeOrigin::root(), dispatched),
				ACTUAL_WEIGHT - <Test as Config>::WeightInfo::set_origin_substrate_tx(),
			);
		});
	}

	#[test]
	fn records_signer_for_the_dispatch() {
		ExtBuilder::default().build().execute_with(|| {
			run(SetOrigin::default(), RuntimeOrigin::signed(ALICE), || {
				assert_eq!(SubstrateTxSigner::<Test>::get(), Some(ALICE));
				dispatched()
			});
			assert!(!SubstrateTxSigner::<Test>::exists());

			run(SetOrigin::new_from_eth_transaction(), RuntimeOrigin::signed(ALICE), || {
				assert!(!SubstrateTxSigner::<Test>::exists());
				dispatched()
			});
		});
	}

	#[test]
	fn nested_transaction_restores_outer_signer() {
		ExtBuilder::default().build().execute_with(|| {
			run(SetOrigin::default(), RuntimeOrigin::signed(ALICE), || {
				run(SetOrigin::default(), RuntimeOrigin::signed(BOB), || {
					assert_eq!(SubstrateTxSigner::<Test>::get(), Some(BOB));
					dispatched()
				});
				assert_eq!(SubstrateTxSigner::<Test>::get(), Some(ALICE));

				// A deploy by the inner signer takes the record, the outer one is still restored.
				run(SetOrigin::default(), RuntimeOrigin::signed(BOB), || {
					SubstrateTxSigner::<Test>::kill();
					dispatched()
				});
				assert_eq!(SubstrateTxSigner::<Test>::get(), Some(ALICE));

				// The same signer nested in its own transaction no longer owns the outer nonce.
				run(SetOrigin::default(), RuntimeOrigin::signed(ALICE), dispatched);
				assert!(!SubstrateTxSigner::<Test>::exists());
				dispatched()
			});
		});
	}
}
