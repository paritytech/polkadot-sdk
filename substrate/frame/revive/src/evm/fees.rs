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
	BalanceOf, Config, IsType,
};
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	pallet_prelude::Weight,
	traits::Get,
	weights::WeightToFee,
};
use frame_system::Config as SysConfig;
use num_traits::Zero;
use pallet_transaction_payment::{Config as TxConfig, MultiplierUpdate, NextFeeMultiplier};
use sp_runtime::{
	generic::UncheckedExtrinsic,
	traits::{Block as BlockT, Dispatchable, TransactionExtension, UniqueSaturatedFrom},
	FixedPointNumber, FixedU128, SaturatedConversion, Saturating,
};

/// The only [`WeightToFee`] implementation that is supported by this pallet.
///
/// `P,Q`: Rational number that defines the ref_time to fee mapping.
///
/// This enforces a ration of ref_time and proof_time that is proportional
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

/// A that signals that [`BlockRatioFee`] is used by the runtime.
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
	/// Needed when deviding a fee by the multiplier before presenting
	/// it to the eth wallet as gas. Needed because the wallet will multiply
	/// it with the gas_price which includes this multiplicator.
	fn next_fee_multiplier_reciprocal() -> FixedU128 {
		Self::next_fee_multiplier()
			.reciprocal()
			.expect("The minimum multiplier is not 0. We check that in `integrity_test`; qed")
	}

	/// Calculate the fee of a transaction without adjusting it using the next fee multiplier.
	///
	/// This also devides the length fee and the base fee by the next fee multiplier
	/// for presentation to the eth wallet.
	fn unadjusted_tx_fee(
		_eth_transact_call: <T as Config>::RuntimeCall,
		_dispatch_call: <T as Config>::RuntimeCall,
	) -> BalanceOf<T> {
		Zero::zero()
	}

	/// Calculate the fee of a transaction including the next fee multiplier adjustment.
	fn tx_fee(_len: u32, _dispatch_info: &DispatchInfo) -> BalanceOf<T> {
		Zero::zero()
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

	/// The hold that storage deposits are collected from when `eth_*` transactions are used.
	fn deposit_source() -> Option<T::RuntimeHoldReason> {
		None
	}
}

impl<const P: u128, const Q: u128, T: Config> BlockRatioWeightToFee for BlockRatioFee<P, Q, T> {
	type T = T;
	const REF_TIME_TO_FEE: FixedU128 = FixedU128::from_rational(P, Q);
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
	<E::Config as SysConfig>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<<E::Config as SysConfig>::Block as BlockT>::Extrinsic: From<
		UncheckedExtrinsic<Address, <E::Config as Config>::RuntimeCall, Signature, E::Extension>,
	>,
	<E::Config as Config>::RuntimeCall: IsType<<E::Config as SysConfig>::RuntimeCall>,
	<E::Config as Config>::RuntimeHoldReason: From<pallet_transaction_payment::HoldReason>,
	<E::Config as TxConfig>::WeightToFee: BlockRatioWeightToFee<T = E::Config>,
	u64: UniqueSaturatedFrom<BalanceOf<E::Config>>,
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

	fn unadjusted_tx_fee(
		eth_transact_call: <E::Config as Config>::RuntimeCall,
		dispatch_call: <E::Config as Config>::RuntimeCall,
	) -> BalanceOf<E::Config> {
		// Get the dispatch info of the actual call dispatched
		let mut dispatch_info = dispatch_call.get_dispatch_info();
		dispatch_info.extension_weight =
			E::get_eth_extension(0u32.into(), 0u32.into()).weight(dispatch_call.into_ref());

		// Build the extrinsic
		let uxt: <<E::Config as SysConfig>::Block as BlockT>::Extrinsic =
			UncheckedExtrinsic::new_bare(eth_transact_call).into();

		// Compute the fee of the extrinsic
		pallet_transaction_payment::Pallet::<E::Config>::compute_unadjusted_fee(
			uxt.encoded_size() as u32,
			&dispatch_info,
		)
		.unwrap()
		.into()
	}

	fn tx_fee(len: u32, dispatch_info: &DispatchInfo) -> BalanceOf<E::Config> {
		pallet_transaction_payment::Pallet::<E::Config>::compute_fee(
			len,
			dispatch_info,
			0u32.into(),
		)
		.into()
	}

	fn weight_to_fee(weight: Weight) -> BalanceOf<E::Config> {
		pallet_transaction_payment::Pallet::<E::Config>::weight_to_fee(weight).into()
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
		pallet_transaction_payment::Pallet::<E::Config>::length_to_fee(len).into()
	}

	fn deposit_source() -> Option<<E::Config as Config>::RuntimeHoldReason> {
		Some(pallet_transaction_payment::HoldReason::Payment.into())
	}
}

impl<T: Config> InfoT<T> for () {}

mod seal {
	pub trait Sealed {}
	impl<const P: u128, const Q: u128, T: super::Config> Sealed for super::BlockRatioFee<P, Q, T> {}
	impl<Address, Signature, E: super::EthExtra> Sealed for super::Info<Address, Signature, E> {}
	impl Sealed for () {}
}
