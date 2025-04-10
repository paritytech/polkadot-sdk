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

use codec::Decode;
use core::{marker::PhantomData, result::Result};
use frame_support::{
	dispatch::GetDispatchInfo,
	traits::{
		fungible::{Balanced, Credit, Inspect},
		Get, OnUnbalanced as OnUnbalancedT,
	},
	weights::{
		constants::{WEIGHT_PROOF_SIZE_PER_MB, WEIGHT_REF_TIME_PER_SECOND},
		WeightToFee as WeightToFeeT,
	},
};
use sp_runtime::traits::{SaturatedConversion, Saturating, Zero};
use xcm::latest::{prelude::*, GetWeight, Weight};
use xcm_executor::{
	traits::{WeightBounds, WeightFee, WeightTrader},
	AssetsInHolding,
};

pub struct FixedWeightBounds<T, C, M>(PhantomData<(T, C, M)>);
impl<T: Get<Weight>, C: Decode + GetDispatchInfo, M: Get<u32>> WeightBounds<C>
	for FixedWeightBounds<T, C, M>
{
	fn weight(message: &mut Xcm<C>) -> Result<Weight, ()> {
		tracing::trace!(target: "xcm::weight", ?message, "FixedWeightBounds");
		let mut instructions_left = M::get();
		Self::weight_with_limit(message, &mut instructions_left)
	}
	fn instr_weight(instruction: &mut Instruction<C>) -> Result<Weight, ()> {
		Self::instr_weight_with_limit(instruction, &mut u32::max_value())
	}
}

impl<T: Get<Weight>, C: Decode + GetDispatchInfo, M> FixedWeightBounds<T, C, M> {
	fn weight_with_limit(message: &mut Xcm<C>, instrs_limit: &mut u32) -> Result<Weight, ()> {
		let mut r: Weight = Weight::zero();
		*instrs_limit = instrs_limit.checked_sub(message.0.len() as u32).ok_or(())?;
		for instruction in message.0.iter_mut() {
			r = r
				.checked_add(&Self::instr_weight_with_limit(instruction, instrs_limit)?)
				.ok_or(())?;
		}
		Ok(r)
	}
	fn instr_weight_with_limit(
		instruction: &mut Instruction<C>,
		instrs_limit: &mut u32,
	) -> Result<Weight, ()> {
		let instr_weight = match instruction {
			Transact { ref mut call, .. } => call.ensure_decoded()?.get_dispatch_info().call_weight,
			SetErrorHandler(xcm) | SetAppendix(xcm) | ExecuteWithOrigin { xcm, .. } =>
				Self::weight_with_limit(xcm, instrs_limit)?,
			_ => Weight::zero(),
		};
		T::get().checked_add(&instr_weight).ok_or(())
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
	fn weight(message: &mut Xcm<C>) -> Result<Weight, ()> {
		tracing::trace!(target: "xcm::weight", ?message, "WeightInfoBounds");
		let mut instructions_left = M::get();
		Self::weight_with_limit(message, &mut instructions_left)
	}
	fn instr_weight(instruction: &mut Instruction<C>) -> Result<Weight, ()> {
		Self::instr_weight_with_limit(instruction, &mut u32::max_value())
	}
}

impl<W, C, M> WeightInfoBounds<W, C, M>
where
	W: XcmWeightInfo<C>,
	C: Decode + GetDispatchInfo,
	M: Get<u32>,
	Instruction<C>: xcm::latest::GetWeight<W>,
{
	fn weight_with_limit(message: &mut Xcm<C>, instrs_limit: &mut u32) -> Result<Weight, ()> {
		let mut r: Weight = Weight::zero();
		*instrs_limit = instrs_limit.checked_sub(message.0.len() as u32).ok_or(())?;
		for instruction in message.0.iter_mut() {
			r = r
				.checked_add(&Self::instr_weight_with_limit(instruction, instrs_limit)?)
				.ok_or(())?;
		}
		Ok(r)
	}
	fn instr_weight_with_limit(
		instruction: &mut Instruction<C>,
		instrs_limit: &mut u32,
	) -> Result<Weight, ()> {
		let instr_weight = match instruction {
			Transact { ref mut call, .. } => call.ensure_decoded()?.get_dispatch_info().call_weight,
			SetErrorHandler(xcm) | SetAppendix(xcm) => Self::weight_with_limit(xcm, instrs_limit)?,
			_ => Weight::zero(),
		};
		instruction.weight().checked_add(&instr_weight).ok_or(())
	}
}

/// Function trait for handling some revenue. Similar to a negative imbalance (credit) handler, but
/// for a `Asset`. Sensible implementations will deposit the asset in some known treasury or
/// block-author account.
pub trait TakeRevenue {
	/// Do something with the given `revenue`, which is a single non-wildcard `Asset`.
	fn take_revenue(revenue: Asset);
}

/// Null implementation just burns the revenue.
impl TakeRevenue for () {
	fn take_revenue(_revenue: Asset) {}
}

/// Simple fee calculator that requires payment in a single fungible at a fixed rate.
///
/// The constant `Get` type parameter should be the fungible ID, the amount of it required for one
/// second of weight and the amount required for 1 MB of proof.
pub struct FixedRateOfFungible<T: Get<(AssetId, u128, u128)>, R: TakeRevenue>(PhantomData<(T, R)>);
impl<T: Get<(AssetId, u128, u128)>, R: TakeRevenue> WeightTrader for FixedRateOfFungible<T, R> {
	fn weight_fee(
		weight: &Weight,
		asset_id: &AssetId,
		context: Option<&XcmContext>,
	) -> Result<WeightFee, XcmError> {
		let (id, units_per_second, units_per_mb) = T::get();
		tracing::trace!(
			target: "xcm::weight",
			?id, ?weight, ?asset_id, ?context,
			"FixedRateOfFungible::weight_price",
		);

		if id.ne(asset_id) {
			return Err(XcmError::FeesNotMet);
		}

		let amount = (units_per_second * (weight.ref_time() as u128) /
			(WEIGHT_REF_TIME_PER_SECOND as u128)) +
			(units_per_mb * (weight.proof_size() as u128) / (WEIGHT_PROOF_SIZE_PER_MB as u128));
		Ok(WeightFee::Desired(amount))
	}

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool {
		let (id, _, _) = T::get();

		if id.ne(asset_id) {
			return false;
		}

		if amount != 0 {
			R::take_revenue((id, amount).into());
		}

		true
	}
}

/// Weight trader which uses the configured `WeightToFee` to set the right price for weight and then
/// places any weight bought into the right account.
pub struct UsingComponents<
	WeightToFee: WeightToFeeT<Balance = <Fungible as Inspect<AccountId>>::Balance>,
	AssetLocation: Get<Location>,
	AccountId,
	Fungible: Balanced<AccountId> + Inspect<AccountId>,
	OnUnbalanced: OnUnbalancedT<Credit<AccountId, Fungible>>,
>(PhantomData<(WeightToFee, AssetLocation, AccountId, Fungible, OnUnbalanced)>);
impl<
		WeightToFee: WeightToFeeT<Balance = <Fungible as Inspect<AccountId>>::Balance>,
		AssetLocation: Get<Location>,
		AccountId,
		Fungible: Balanced<AccountId> + Inspect<AccountId>,
		OnUnbalanced: OnUnbalancedT<Credit<AccountId, Fungible>>,
	> WeightTrader for UsingComponents<WeightToFee, AssetLocation, AccountId, Fungible, OnUnbalanced>
{
	fn weight_fee(
		weight: &Weight,
		asset_id: &AssetId,
		context: Option<&XcmContext>,
	) -> Result<WeightFee, XcmError> {
		let required_asset_id = AssetId(AssetLocation::get());
		if required_asset_id.ne(asset_id) {
			return Err(XcmError::FeesNotMet);
		}

		tracing::trace!(target: "xcm::weight", ?weight, ?asset_id, ?context, "UsingComponents::weight_price");
		let amount = WeightToFee::weight_to_fee(&weight);
		let required_amount: u128 = amount.try_into().map_err(|_| {
			tracing::debug!(target: "xcm::weight", ?amount, "Weight fee could not be converted");
			XcmError::Overflow
		})?;

		Ok(WeightFee::Desired(required_amount))
	}

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool {
		if AssetLocation::get().ne(&asset_id.0) {
			return false;
		}

		let Ok(amount) = amount.try_into() else {
			// FIXME log
			// TODO justify why this should be impossible and use defensive!
			tracing::debug!(target: "xcm::weight", ?amount, "Weight fee could not be converted for depositing");
			return false;
		};

		OnUnbalanced::on_unbalanced(Fungible::issue(amount));
		true
	}
}
