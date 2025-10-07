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

//! Contains the fee types that need to be configured for `pallet-transaction-payment`.

use crate::{
	evm::{runtime::EthExtra, OnChargeTransactionBalanceOf},
	BalanceOf, CallOf, Config, DispatchErrorWithPostInfo, DispatchResultWithPostInfo, Error,
	PostDispatchInfo, LOG_TARGET,
};
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	pallet_prelude::Weight,
	traits::{fungible::Credit, Get, SuppressedDrop},
	weights::WeightToFee,
};
use frame_system::Config as SysConfig;
use num_traits::Zero;
use pallet_transaction_payment::{
	Config as TxConfig, MultiplierUpdate, NextFeeMultiplier, Pallet as TxPallet, TxCreditHold,
};
use sp_runtime::{
	generic::UncheckedExtrinsic,
	traits::{Block as BlockT, Dispatchable, TransactionExtension},
	FixedPointNumber, FixedU128, SaturatedConversion, Saturating,
};

type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// The only [`WeightToFee`] implementation that is supported by this pallet.
///
/// `P,Q`: Rational number that defines the ref_time to fee mapping.
///
/// This enforces a ratio of ref_time and proof_time that is proportional
/// to their distribution in the block limits. We enforce the usage of this fee
/// structure because our gas mapping depends on it.
///
/// # Panics
///
/// If either `P` or `Q` is zero.
pub struct BlockRatioFee<const P: u128, const Q: u128, T: Config>(PhantomData<T>);

/// The only [`InfoT`] implementation valid for [`Config::FeeInfo`].
///
/// The reason for this type is to avoid coupling the rest of pallet_revive to
/// pallet_transaction_payment. This way we bundle all the trait bounds in once place.
pub struct Info<Address, Signature, Extra>(PhantomData<(Address, Signature, Extra)>);

/// A trait that signals that [`BlockRatioFee`] is used by the runtime.
///
/// This trait is sealed. Use [`BlockRatioFee`].
pub trait BlockRatioWeightToFee: seal::Sealed {
	/// The runtime.
	type T: Config;
	/// The ref_time to fee coefficient.
	const REF_TIME_TO_FEE: FixedU128;

	/// The proof_size to fee coefficient.
	fn proof_size_to_fee() -> FixedU128 {
		let max_weight = <Self::T as frame_system::Config>::BlockWeights::get().max_block;
		let ratio =
			FixedU128::from_rational(max_weight.ref_time().into(), max_weight.proof_size().into());
		Self::REF_TIME_TO_FEE.saturating_mul(ratio)
	}
}

/// A trait that exposes all the transaction payment details to `pallet_revive`.
///
/// This trait is sealed. Use [`Info`].
pub trait InfoT<T: Config>: seal::Sealed {
	/// Check that the fee configuration of the chain is valid.
	///
	/// This is being called by the pallets `integrity_check`.
	fn integrity_test() {}

	/// Exposes the current fee multiplier of the chain.
	fn next_fee_multiplier() -> FixedU128 {
		FixedU128::from_rational(1, 1)
	}

	/// The reciprocal of the next fee multiplier.
	///
	/// Needed when dividing a fee by the multiplier before presenting
	/// it to the eth wallet as gas. Needed because the wallet will multiply
	/// it with the gas_price which includes this multiplicator.
	fn next_fee_multiplier_reciprocal() -> FixedU128 {
		Self::next_fee_multiplier()
			.reciprocal()
			.expect("The minimum multiplier is not 0. We check that in `integrity_test`; qed")
	}

	/// Calculate the fee of a transaction including the next fee multiplier adjustment.
	fn tx_fee(_len: u32, _call: &CallOf<T>) -> BalanceOf<T> {
		Zero::zero()
	}

	/// Makes sure that not too much storage deposit was withdrawn.
	fn ensure_not_overdrawn(
		_encoded_len: u32,
		_info: &DispatchInfo,
		result: DispatchResultWithPostInfo,
	) -> DispatchResultWithPostInfo {
		result
	}

	/// Get the dispatch info of a call with the proper extension weight set.
	fn dispatch_info(_call: &CallOf<T>) -> DispatchInfo {
		Default::default()
	}

	/// Calculate the encoded length of a call.
	fn encoded_len(_eth_transact_call: CallOf<T>) -> u32 {
		0
	}

	/// Convert a weight to an unadjusted fee.
	fn weight_to_fee(_weight: Weight) -> BalanceOf<T> {
		Zero::zero()
	}

	/// Convert an unadjusted fee back to a weight.
	fn fee_to_weight(_fee: BalanceOf<T>) -> Weight {
		Zero::zero()
	}

	/// Convert the length of a transaction to an unadjusted weight.
	fn length_to_fee(_len: u32) -> BalanceOf<T> {
		Zero::zero()
	}

	/// Add some additional fee to the `pallet_transaction_payment` credit.
	fn deposit_txfee(_credit: CreditOf<T>) {}

	/// Withdraw some fee to pay for storage deposits.
	fn withdraw_txfee(_amount: BalanceOf<T>) -> Option<CreditOf<T>> {
		Default::default()
	}

	/// Return the remaining transaction fee.
	fn remaining_txfee() -> BalanceOf<T> {
		Default::default()
	}
}

impl<const P: u128, const Q: u128, T: Config> BlockRatioWeightToFee for BlockRatioFee<P, Q, T> {
	type T = T;
	const REF_TIME_TO_FEE: FixedU128 = {
		assert!(P > 0 && Q > 0);
		FixedU128::from_rational(P, Q)
	};
}

impl<const P: u128, const Q: u128, T: Config> WeightToFee for BlockRatioFee<P, Q, T> {
	type Balance = BalanceOf<T>;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		let ref_time_fee = Self::REF_TIME_TO_FEE
			.saturating_mul_int(Self::Balance::saturated_from(weight.ref_time()));
		let proof_size_fee = Self::proof_size_to_fee()
			.saturating_mul_int(Self::Balance::saturated_from(weight.proof_size()));
		ref_time_fee.max(proof_size_fee)
	}
}

impl<Address, Signature, E: EthExtra> InfoT<E::Config> for Info<Address, Signature, E>
where
	BalanceOf<E::Config>: From<OnChargeTransactionBalanceOf<E::Config>>,
	<E::Config as frame_system::Config>::RuntimeCall:
		Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	<<E::Config as SysConfig>::Block as BlockT>::Extrinsic:
		From<UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>>,
	<E::Config as TxConfig>::WeightToFee: BlockRatioWeightToFee<T = E::Config>,
	<<E::Config as TxConfig>::OnChargeTransaction as TxCreditHold<E::Config>>::Credit:
		SuppressedDrop<Inner = CreditOf<E::Config>>,
{
	fn integrity_test() {
		let min_multiplier = <E::Config as TxConfig>::FeeMultiplierUpdate::min();
		assert!(!min_multiplier.is_zero(), "The multiplier is never allowed to be zero.");
		assert!(
			min_multiplier.saturating_mul_int(<E::Config as Config>::NativeToEthRatio::get()) > 0,
			"The gas price needs to be greater zero."
		);
		assert!(
			!<E::Config as TxConfig>::WeightToFee::REF_TIME_TO_FEE.is_zero(),
			"ref_time to fee is not allowed to be zero."
		);
		assert!(
			!<E::Config as TxConfig>::WeightToFee::proof_size_to_fee().is_zero(),
			"proof_size to fee is not allowed to be zero."
		);
	}

	fn next_fee_multiplier() -> FixedU128 {
		<NextFeeMultiplier<E::Config>>::get()
	}

	fn tx_fee(len: u32, call: &CallOf<E::Config>) -> BalanceOf<E::Config> {
		let dispatch_info = Self::dispatch_info(call);
		TxPallet::<E::Config>::compute_fee(len, &dispatch_info, 0u32.into()).into()
	}

	fn ensure_not_overdrawn(
		encoded_len: u32,
		info: &DispatchInfo,
		result: DispatchResultWithPostInfo,
	) -> DispatchResultWithPostInfo {
		// if tx is already failing we can ignore
		// as it will be rolled back anyways
		let Ok(post_info) = result else {
			return result;
		};

		let fee: BalanceOf<E::Config> =
			<TxPallet<E::Config>>::compute_actual_fee(encoded_len, info, &post_info, Zero::zero())
				.into();
		let available = Self::remaining_txfee();
		if fee > available {
			log::debug!(target: LOG_TARGET, "Drew too much from the txhold. \
				fee={fee:?} \
				available={available:?} \
				overdrawn_by={:?}",
				fee.saturating_sub(available),
			);
			Err(DispatchErrorWithPostInfo {
				post_info,
				error: <Error<E::Config>>::TxFeeOverdraw.into(),
			})
		} else {
			log::trace!(target: LOG_TARGET, "Enough left in the txhold. \
				fee={fee:?} \
				available={available:?} \
				refund={:?}",
				available.saturating_sub(fee),
			);
			result
		}
	}

	fn dispatch_info(call: &CallOf<E::Config>) -> DispatchInfo {
		let mut dispatch_info = call.get_dispatch_info();
		dispatch_info.extension_weight =
			E::get_eth_extension(0u32.into(), 0u32.into()).weight(call);
		dispatch_info
	}

	fn encoded_len(eth_transact_call: CallOf<E::Config>) -> u32 {
		let uxt: <<E::Config as SysConfig>::Block as BlockT>::Extrinsic =
			UncheckedExtrinsic::new_bare(eth_transact_call).into();
		uxt.encoded_size() as u32
	}

	fn weight_to_fee(weight: Weight) -> BalanceOf<E::Config> {
		TxPallet::<E::Config>::weight_to_fee(weight).into()
	}

	/// Convert an unadjusted fee back to a weight.
	fn fee_to_weight(fee: BalanceOf<E::Config>) -> Weight {
		let ref_time = <E::Config as TxConfig>::WeightToFee::REF_TIME_TO_FEE
			.reciprocal()
			.expect("This is not zero. Enforced in `integrity_test`; qed")
			.saturating_mul_int(fee);
		let proof_size = <E::Config as TxConfig>::WeightToFee::proof_size_to_fee()
			.reciprocal()
			.expect("This is not zero. Enforced in `integrity_test`; qed")
			.saturating_mul_int(fee);
		Weight::from_parts(ref_time.saturated_into(), proof_size.saturated_into())
	}

	fn length_to_fee(len: u32) -> BalanceOf<E::Config> {
		TxPallet::<E::Config>::length_to_fee(len).into()
	}

	fn deposit_txfee(credit: CreditOf<E::Config>) {
		TxPallet::<E::Config>::deposit_txfee(credit)
	}

	fn withdraw_txfee(amount: BalanceOf<E::Config>) -> Option<CreditOf<E::Config>> {
		TxPallet::<E::Config>::withdraw_txfee(amount)
	}

	fn remaining_txfee() -> BalanceOf<E::Config> {
		TxPallet::<E::Config>::remaining_txfee()
	}
}

impl<T: Config> InfoT<T> for () {}

mod seal {
	pub trait Sealed {}
	impl<const P: u128, const Q: u128, T: super::Config> Sealed for super::BlockRatioFee<P, Q, T> {}
	impl<Address, Signature, E: super::EthExtra> Sealed for super::Info<Address, Signature, E> {}
	impl Sealed for () {}
}
