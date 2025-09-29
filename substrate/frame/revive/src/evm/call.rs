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

//! Functionality to decode an eth transaction into an dispatchable call.

use crate::{
	evm::{fees::InfoT, runtime::SetWeightLimit},
	extract_code_and_data, BalanceOf, Call, CallOf, Config, DispatchClass, GenericTransaction,
	MomentOf, Pallet, Weight, Zero, H256, LOG_TARGET, RUNTIME_PALLETS_ADDR, U256,
};
use codec::DecodeLimit;
use frame_support::MAX_EXTRINSIC_DEPTH;
use num_traits::Bounded;
use sp_core::Get;
use sp_runtime::{
	transaction_validity::InvalidTransaction, FixedPointNumber, SaturatedConversion, Saturating,
};

pub struct CallInfo<T: Config> {
	pub call: CallOf<T>,
	pub weight_limit: Weight,
	pub encoded_len: u32,
	pub storage_deposit: BalanceOf<T>,
}

pub fn create_call<T>(
	tx: GenericTransaction,
	encoded_len: Option<u32>,
) -> Result<CallInfo<T>, InvalidTransaction>
where
	T: Config,
	T::Hash: frame_support::traits::IsType<H256>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	CallOf<T>: From<Call<T>> + SetWeightLimit,
{
	let base_fee = <Pallet<T>>::evm_gas_price();

	let Some(gas) = tx.gas else {
		log::debug!(target: LOG_TARGET, "No gas provided");
		return Err(InvalidTransaction::Call);
	};

	let Some(effective_gas_price) = tx.gas_price else {
		log::debug!(target: LOG_TARGET, "No gas_price provided.");
		return Err(InvalidTransaction::Payment);
	};

	let chain_id = tx.chain_id.unwrap_or_default();

	if chain_id != <T as Config>::ChainId::get().into() {
		log::debug!(target: LOG_TARGET, "Invalid chain_id {chain_id:?}");
		return Err(InvalidTransaction::Call);
	}

	if effective_gas_price < base_fee {
		log::debug!(
			target: LOG_TARGET,
			"Specified gas_price is too low. effective_gas_price={effective_gas_price} base_fee={base_fee}"
		);
		return Err(InvalidTransaction::Payment);
	}

	let encoded_len = if let Some(encoded_len) = encoded_len {
		encoded_len
	} else {
		let unsigned_tx = tx.clone().try_into_unsigned().map_err(|_| {
			log::debug!(target: LOG_TARGET, "Not enough gas supplied to cover the weight of the extrinsic.");
			InvalidTransaction::Payment
		})?;
		let eth_transact_call =
			crate::Call::<T>::eth_transact { payload: unsigned_tx.dummy_signed_payload() };
		<T as Config>::FeeInfo::encoded_len(eth_transact_call.into())
	};

	let value = tx.value.unwrap_or_default();
	let data = tx.input.to_vec();

	let mut call = if let Some(dest) = tx.to {
		if dest == RUNTIME_PALLETS_ADDR {
			let call =
				CallOf::<T>::decode_all_with_depth_limit(MAX_EXTRINSIC_DEPTH, &mut &data[..])
					.map_err(|_| {
						log::debug!(target: LOG_TARGET, "Failed to decode data as Call");
						InvalidTransaction::Call
					})?;

			if !value.is_zero() {
				log::debug!(target: LOG_TARGET, "Runtime pallets address cannot be called with value");
				return Err(InvalidTransaction::Call)
			}

			call
		} else {
			let call = crate::Call::eth_call::<T> {
				dest,
				value,
				gas_limit: Zero::zero(),
				storage_deposit_limit: BalanceOf::<T>::max_value(),
				data,
				effective_gas_price,
			}
			.into();
			call
		}
	} else {
		let (code, data) = if data.starts_with(&polkavm_common::program::BLOB_MAGIC) {
			let Some((code, data)) = extract_code_and_data(&data) else {
				log::debug!(target: LOG_TARGET, "Failed to extract polkavm code & data");
				return Err(InvalidTransaction::Call);
			};
			(code, data)
		} else {
			(data, Default::default())
		};

		let call = crate::Call::eth_instantiate_with_code::<T> {
			value,
			gas_limit: Zero::zero(),
			storage_deposit_limit: BalanceOf::<T>::max_value(),
			code,
			data,
			effective_gas_price,
		}
		.into();

		call
	};

	// the fee as signed off by the eth wallet. we cannot consume more.
	let eth_fee = effective_gas_price.saturating_mul(gas) / <T as Config>::NativeToEthRatio::get();

	let weight_limit = {
		let info = <T as Config>::FeeInfo::dispatch_info(&call);
		let fixed_fee = <T as Config>::FeeInfo::weight_to_fee(
			<T as frame_system::Config>::BlockWeights::get().get(info.class).base_extrinsic,
		)
		.saturating_add(<T as Config>::FeeInfo::length_to_fee(encoded_len as u32));

		let remaining_fee = {
			let adjusted = eth_fee.checked_sub(fixed_fee.into()).ok_or_else(|| {
							log::debug!(target: LOG_TARGET, "Not enough gas supplied to cover base and len fee. eth_fee={eth_fee:?} fixed_fee={fixed_fee:?}");
							InvalidTransaction::Payment
						})?;
			let unadjusted = <T as Config>::FeeInfo::next_fee_multiplier_reciprocal()
				.saturating_mul_int(<BalanceOf<T>>::saturated_from(adjusted));
			unadjusted
		};

		let weight_limit = <T as Config>::FeeInfo::fee_to_weight(remaining_fee)
						.checked_sub(&info.total_weight()).ok_or_else(|| {
							log::debug!(target: LOG_TARGET, "Not enough gas supplied to cover the weight of the extrinsic.");
							InvalidTransaction::Payment
						})?;

		call.set_weight_limit(weight_limit);
		let info = <T as Config>::FeeInfo::dispatch_info(&call);

		let factor = <T as Config>::MaxEthExtrinsicWeight::get();
		let max_weight = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or_else(|| <T as frame_system::Config>::BlockWeights::get().max_block);
		let max_weight = Weight::from_parts(
			factor.saturating_mul_int(max_weight.ref_time()),
			factor.saturating_mul_int(max_weight.proof_size()),
		);

		let overweight_by = info.total_weight().saturating_sub(max_weight);
		let capped_weight = weight_limit.saturating_sub(overweight_by);
		call.set_weight_limit(capped_weight);
		capped_weight
	};

	// the overall fee of the extrinsic including the gas limit
	let tx_fee = <T as Config>::FeeInfo::tx_fee(encoded_len, &call);

	// the leftover we make available to the deposit collection system
	let storage_deposit = eth_fee.checked_sub(tx_fee.into()).ok_or_else(|| {
		log::error!(target: LOG_TARGET, "The eth_fee={eth_fee:?} is smaller than the tx_fee={tx_fee:?}. This is a bug.");
		InvalidTransaction::Payment
	})?.saturated_into();

	Ok(CallInfo { call, weight_limit, encoded_len, storage_deposit })
}
