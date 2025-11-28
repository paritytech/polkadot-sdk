// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use alloc::boxed::Box;
use codec::Decode;
use core::{marker::PhantomData, result::Result};
use frame_support::{
	dispatch::GetDispatchInfo,
	traits::{
		fungible::{Balanced, Credit, Imbalance, Inspect},
		tokens::imbalance::{ImbalanceAccounting, UnsafeManualAccounting},
		Get, Imbalance as ImbalanceT, OnUnbalanced as OnUnbalancedT,
	},
	weights::{
		constants::{WEIGHT_PROOF_SIZE_PER_MB, WEIGHT_REF_TIME_PER_SECOND},
		WeightToFee as WeightToFeeT,
	},
};
use sp_runtime::traits::Zero;
use xcm::latest::{prelude::*, GetWeight, Weight};
use xcm_executor::{
	traits::{WeightBounds, WeightTrader},
	AssetsInHolding,
};

pub struct FixedWeightBounds<T, C, M>(PhantomData<(T, C, M)>);
impl<T: Get<Weight>, C: Decode + GetDispatchInfo, M: Get<u32>> WeightBounds<C>
	for FixedWeightBounds<T, C, M>
{
	fn weight(message: &mut Xcm<C>, weight_limit: Weight) -> Result<Weight, InstructionError> {
		tracing::trace!(target: "xcm::weight", ?message, "FixedWeightBounds");
		let mut instructions_left = M::get();
		Self::weight_with_limit(message, &mut instructions_left, weight_limit).inspect_err(
			|&error| {
				tracing::debug!(
					target: "xcm::weight",
					?error,
					?instructions_left,
					message_length = ?message.0.len(),
					"Weight calculation failed for message"
				);
			},
		)
	}
	fn instr_weight(instruction: &mut Instruction<C>) -> Result<Weight, XcmError> {
		let mut max_value = u32::MAX;
		Self::instr_weight_with_limit(instruction, &mut max_value, Weight::MAX).inspect_err(
			|&error| {
				tracing::debug!(
					target: "xcm::weight",
					?error,
					?instruction,
					instrs_limit = ?max_value,
					"Weight calculation failed for instruction"
				);
			},
		)
	}
}

impl<T: Get<Weight>, C: Decode + GetDispatchInfo, M> FixedWeightBounds<T, C, M> {
	fn weight_with_limit(
		message: &mut Xcm<C>,
		instructions_left: &mut u32,
		weight_limit: Weight,
	) -> Result<Weight, InstructionError> {
		let mut total_weight: Weight = Weight::zero();
		for (index, instruction) in message.0.iter_mut().enumerate() {
			let index = index.try_into().unwrap_or(InstructionIndex::MAX);
			*instructions_left = instructions_left
				.checked_sub(1)
				.ok_or_else(|| InstructionError { index, error: XcmError::ExceedsStackLimit })?;
			let instruction_weight =
				&Self::instr_weight_with_limit(instruction, instructions_left, weight_limit)
					.map_err(|error| InstructionError { index, error })?;
			total_weight = total_weight
				.checked_add(instruction_weight)
				.ok_or(InstructionError { index, error: XcmError::Overflow })?;
			if total_weight.any_gt(weight_limit) {
				return Err(InstructionError {
					index,
					error: XcmError::WeightLimitReached(total_weight),
				});
			}
		}
		Ok(total_weight)
	}

	fn instr_weight_with_limit(
		instruction: &mut Instruction<C>,
		instructions_left: &mut u32,
		weight_limit: Weight,
	) -> Result<Weight, XcmError> {
		let instruction_weight = match instruction {
			Transact { ref mut call, .. } =>
				call.ensure_decoded()
					.map_err(|_| XcmError::FailedToDecode)?
					.get_dispatch_info()
					.call_weight,
			SetErrorHandler(xcm) | SetAppendix(xcm) | ExecuteWithOrigin { xcm, .. } =>
				Self::weight_with_limit(xcm, instructions_left, weight_limit)
					.map_err(|outcome_error| outcome_error.error)?,
			_ => Weight::zero(),
		};
		let total_weight = T::get().checked_add(&instruction_weight).ok_or(XcmError::Overflow)?;
		Ok(total_weight)
	}
}

pub struct WeightInfoBounds<W, C, M>(PhantomData<(W, C, M)>);
impl<W, C, M> WeightBounds<C> for WeightInfoBounds<W, C, M>
where
	W: XcmWeightInfo<C>,
	C: Decode + GetDispatchInfo,
	M: Get<u32>,
	Instruction<C>: xcm::latest::GetWeight<W>,
{
	fn weight(message: &mut Xcm<C>, weight_limit: Weight) -> Result<Weight, InstructionError> {
		tracing::trace!(target: "xcm::weight", ?message, "WeightInfoBounds");
		let mut instructions_left = M::get();
		Self::weight_with_limit(message, &mut instructions_left, weight_limit).inspect_err(
			|&error| {
				tracing::debug!(
					target: "xcm::weight",
					?error,
					?instructions_left,
					message_length = ?message.0.len(),
					"Weight calculation failed for message"
				);
			},
		)
	}
	fn instr_weight(instruction: &mut Instruction<C>) -> Result<Weight, XcmError> {
		let mut max_value = u32::MAX;
		Self::instr_weight_with_limit(instruction, &mut max_value, Weight::MAX).inspect_err(
			|&error| {
				tracing::debug!(
					target: "xcm::weight",
					?error,
					?instruction,
					instrs_limit = ?max_value,
					"Weight calculation failed for instruction"
				);
			},
		)
	}
}

impl<W, C, M> WeightInfoBounds<W, C, M>
where
	W: XcmWeightInfo<C>,
	C: Decode + GetDispatchInfo,
	M: Get<u32>,
	Instruction<C>: xcm::latest::GetWeight<W>,
{
	fn weight_with_limit(
		message: &mut Xcm<C>,
		instructions_left: &mut u32,
		weight_limit: Weight,
	) -> Result<Weight, InstructionError> {
		let mut total_weight: Weight = Weight::zero();
		for (index, instruction) in message.0.iter_mut().enumerate() {
			let index = index.try_into().unwrap_or(u8::MAX);
			*instructions_left = instructions_left
				.checked_sub(1)
				.ok_or_else(|| InstructionError { index, error: XcmError::ExceedsStackLimit })?;
			let instruction_weight =
				&Self::instr_weight_with_limit(instruction, instructions_left, weight_limit)
					.map_err(|error| InstructionError { index, error })?;
			total_weight = total_weight
				.checked_add(instruction_weight)
				.ok_or(InstructionError { index, error: XcmError::Overflow })?;
			if total_weight.any_gt(weight_limit) {
				return Err(InstructionError {
					index,
					error: XcmError::WeightLimitReached(total_weight),
				});
			}
		}
		Ok(total_weight)
	}

	fn instr_weight_with_limit(
		instruction: &mut Instruction<C>,
		instructions_left: &mut u32,
		weight_limit: Weight,
	) -> Result<Weight, XcmError> {
		let instruction_weight = match instruction {
			Transact { ref mut call, .. } =>
				call.ensure_decoded()
					.map_err(|_| XcmError::FailedToDecode)?
					.get_dispatch_info()
					.call_weight,
			SetErrorHandler(xcm) | SetAppendix(xcm) =>
				Self::weight_with_limit(xcm, instructions_left, weight_limit)
					.map_err(|outcome_error| outcome_error.error)?,
			_ => Weight::zero(),
		};
		let total_weight = instruction
			.weight()
			.checked_add(&instruction_weight)
			.ok_or(XcmError::Overflow)?;
		Ok(total_weight)
	}
}

/// Function trait for handling some revenue. Similar to a negative imbalance (credit) handler, but
/// for a `Asset`. Sensible implementations will deposit the asset in some known treasury or
/// block-author account.
pub trait TakeRevenue {
	/// Do something with the given `revenue`.
	fn take_revenue(revenue: AssetsInHolding);
}

/// Null implementation just burns the revenue (drops imbalance).
impl TakeRevenue for () {
	fn take_revenue(_revenue: AssetsInHolding) {}
}

/// Simple fee calculator that requires payment in a single fungible at a fixed rate.
///
/// The constant `Get` type parameter should be the fungible ID, the amount of it required for one
/// second of weight and the amount required for 1 MB of proof.
pub struct FixedRateOfFungible<T: Get<(AssetId, u128, u128)>, R: TakeRevenue>(
	Weight,
	AssetsInHolding,
	PhantomData<(T, R)>,
);
impl<T: Get<(AssetId, u128, u128)>, R: TakeRevenue> WeightTrader for FixedRateOfFungible<T, R> {
	fn new() -> Self {
		Self(Weight::zero(), AssetsInHolding::new(), PhantomData)
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		mut payment: AssetsInHolding,
		context: &XcmContext,
	) -> Result<AssetsInHolding, (AssetsInHolding, XcmError)> {
		let (id, units_per_second, units_per_mb) = T::get();
		tracing::trace!(
			target: "xcm::weight",
			?id, ?weight, ?payment, ?context,
			"FixedRateOfFungible::buy_weight",
		);
		let amount = (units_per_second * (weight.ref_time() as u128) /
			(WEIGHT_REF_TIME_PER_SECOND as u128)) +
			(units_per_mb * (weight.proof_size() as u128) / (WEIGHT_PROOF_SIZE_PER_MB as u128));
		if amount == 0 {
			return Ok(payment)
		}
		let to_charge: Asset = (id, amount).into();
		if let Ok(taken) = payment.try_take(to_charge.into()) {
			self.0 = self.0.saturating_add(weight);
			self.1.subsume_assets(taken);
			Ok(payment)
		} else {
			Err((payment, XcmError::TooExpensive))
		}
	}

	fn refund_weight(&mut self, weight: Weight, context: &XcmContext) -> Option<AssetsInHolding> {
		let (id, units_per_second, units_per_mb) = T::get();
		tracing::trace!(target: "xcm::weight", ?id, ?weight, ?context, "FixedRateOfFungible::refund_weight");
		let weight = weight.min(self.0);
		let amount = (units_per_second * (weight.ref_time() as u128) /
			(WEIGHT_REF_TIME_PER_SECOND as u128)) +
			(units_per_mb * (weight.proof_size() as u128) / (WEIGHT_PROOF_SIZE_PER_MB as u128));
		self.0 -= weight;
		self.1.fungible.get_mut(&id).and_then(|credit| {
			let refunded = credit.saturating_take(amount);
			if refunded.amount() > 0 {
				Some(AssetsInHolding::new_from_fungible_credit(id, refunded))
			} else {
				None
			}
		})
	}

	fn quote_weight(
		&mut self,
		weight: Weight,
		given: AssetId,
		context: &XcmContext,
	) -> Result<Asset, XcmError> {
		let (id, units_per_second, units_per_mb) = T::get();
		tracing::trace!(
			target: "xcm::weight",
			?id, ?weight, ?given, ?context,
			"FixedRateOfFungible::quote_weight",
		);
		let amount = (units_per_second * (weight.ref_time() as u128) /
			(WEIGHT_REF_TIME_PER_SECOND as u128)) +
			(units_per_mb * (weight.proof_size() as u128) / (WEIGHT_PROOF_SIZE_PER_MB as u128));
		Ok((id, amount).into())
	}
}

impl<T: Get<(AssetId, u128, u128)>, R: TakeRevenue> Drop for FixedRateOfFungible<T, R> {
	fn drop(&mut self) {
		if !self.1.is_empty() {
			let mut taken = AssetsInHolding::new();
			core::mem::swap(&mut self.1, &mut taken);
			R::take_revenue(taken);
		}
	}
}

/// Weight trader which uses the configured `WeightToFee` to set the right price for weight and then
/// places any weight bought into the right account.
pub struct UsingComponents<
	WeightToFee: WeightToFeeT<Balance = <Fungible as Inspect<AccountId>>::Balance>,
	AssetIdValue: Get<Location>,
	AccountId,
	Fungible: Balanced<AccountId> + Inspect<AccountId>,
	OnUnbalanced: OnUnbalancedT<Credit<AccountId, Fungible>>,
>(
	Weight,
	Credit<AccountId, Fungible>,
	PhantomData<(WeightToFee, AssetIdValue, AccountId, Fungible, OnUnbalanced)>,
);
impl<
		WeightToFee: WeightToFeeT<Balance = <Fungible as Inspect<AccountId>>::Balance>,
		AssetIdValue: Get<Location>,
		AccountId,
		Fungible: Balanced<AccountId, OnDropDebt: 'static, OnDropCredit: 'static> + Inspect<AccountId>,
		OnUnbalanced: OnUnbalancedT<Credit<AccountId, Fungible>>,
	> WeightTrader for UsingComponents<WeightToFee, AssetIdValue, AccountId, Fungible, OnUnbalanced>
where
	Imbalance<
		<Fungible as Inspect<AccountId>>::Balance,
		<Fungible as Balanced<AccountId>>::OnDropCredit,
		<Fungible as Balanced<AccountId>>::OnDropDebt,
	>: ImbalanceAccounting<u128>,
{
	fn new() -> Self {
		Self(Weight::zero(), Default::default(), PhantomData)
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		mut payment: AssetsInHolding,
		context: &XcmContext,
	) -> Result<AssetsInHolding, (AssetsInHolding, XcmError)> {
		tracing::trace!(target: "xcm::weight", ?weight, ?payment, ?context, "UsingComponents::buy_weight");
		let amount = WeightToFee::weight_to_fee(&weight);
		let Ok(u128_amount): Result<u128, _> = TryInto::<u128>::try_into(amount) else {
			tracing::debug!(target: "xcm::weight", ?amount, "Weight fee could not be converted");
			return Err((payment, XcmError::Overflow))
		};
		let asset_id = AssetId(AssetIdValue::get());
		let required = Asset { id: asset_id.clone(), fun: Fungible(u128_amount) };
		if let Ok(mut taken) = payment.try_take(required.into()) {
			self.0 = self.0.saturating_add(weight);
			if let Some(imbalance) = taken.fungible.remove(&asset_id) {
				self.1.subsume_other(imbalance);
				Ok(payment)
			} else {
				payment.subsume_assets(taken);
				Err((payment, XcmError::TooExpensive))
			}
		} else {
			Err((payment, XcmError::TooExpensive))
		}
	}

	fn refund_weight(&mut self, weight: Weight, context: &XcmContext) -> Option<AssetsInHolding> {
		tracing::trace!(target: "xcm::weight", ?weight, ?context, available_weight = ?self.0, available_amount = ?self.1, "UsingComponents::refund_weight");
		let weight = weight.min(self.0);
		let amount = WeightToFee::weight_to_fee(&weight);
		self.0 -= weight;
		// self.1 = self.1.saturating_sub(amount);
		let refund = self.1.extract(amount);
		tracing::trace!(target: "xcm::weight", ?amount, "UsingComponents::refund_weight");
		if refund.peek() != Zero::zero() {
			Some(AssetsInHolding::new_from_fungible_credit(
				AssetId(AssetIdValue::get()),
				Box::new(refund),
			))
		} else {
			None
		}
	}

	fn quote_weight(
		&mut self,
		weight: Weight,
		given: AssetId,
		context: &XcmContext,
	) -> Result<Asset, XcmError> {
		tracing::trace!(target: "xcm::weight", ?weight, ?given, ?context, "UsingComponents::quote_weight");
		let amount = WeightToFee::weight_to_fee(&weight);
		let u128_amount: u128 = TryInto::<u128>::try_into(amount).map_err(|_| {
			tracing::debug!(target: "xcm::weight", ?amount, "Weight fee could not be converted");
			XcmError::Overflow
		})?;
		let required = Asset { id: AssetId(AssetIdValue::get()), fun: Fungible(u128_amount) };
		Ok(required)
	}
}
impl<
		WeightToFee: WeightToFeeT<Balance = <Fungible as Inspect<AccountId>>::Balance>,
		AssetId: Get<Location>,
		AccountId,
		Fungible: Balanced<AccountId> + Inspect<AccountId>,
		OnUnbalanced: OnUnbalancedT<Credit<AccountId, Fungible>>,
	> Drop for UsingComponents<WeightToFee, AssetId, AccountId, Fungible, OnUnbalanced>
{
	fn drop(&mut self) {
		if self.1.peek().is_zero() {
			return
		}
		let total_fee = self.1.extract(self.1.peek());
		OnUnbalanced::on_unbalanced(total_fee);
	}
}
