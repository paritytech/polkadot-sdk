//! `FinalRecovery` FIFO operations and settlement-pricing helpers.
//!
//! See `troves.md` §6 (FIFO ops) and §7.6 (recovery pricing).

use crate::{
	pallet::{BranchStates, Config, Error, Event, FinalRecoveryNodes, Pallet},
	types::FinalRecoveryNode,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::{
	frame_support::traits::Time,
	sp_runtime::{
		traits::{CheckedDiv, Saturating, Zero},
		DispatchError, FixedPointNumber, FixedU128,
	},
};
use scale_info::TypeInfo;

/// Pricing regime applied to a `FinalRecovery` redemption / offset.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
)]
pub enum RecoveryPricing {
	/// Vault CR >= 100%; redeemers receive face value plus a bounded bonus.
	BonusAboveOneHundred,
	/// Vault CR < 100%; redeemers receive `recovery_rate * x` collateral.
	InsuranceAdjustedBelowOneHundred,
}

/// Append `owner` to the per-branch FIFO. Errors if already present.
pub fn append<T: Config>(
	collateral_id: &T::AssetId,
	owner: T::AccountId,
) -> Result<(), DispatchError> {
	if FinalRecoveryNodes::<T>::contains_key(collateral_id, &owner) {
		return Err(Error::<T>::FinalRecoveryInvariantBroken.into());
	}

	let now = T::TimeProvider::now();
	BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
		let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		let prev = branch.final_recovery_tail.clone();
		let node = FinalRecoveryNode { prev: prev.clone(), next: None, entered_at: now };
		FinalRecoveryNodes::<T>::insert(collateral_id, &owner, node);

		if let Some(prev_owner) = prev {
			FinalRecoveryNodes::<T>::mutate(collateral_id, &prev_owner, |maybe| {
				if let Some(n) = maybe {
					n.next = Some(owner.clone());
				}
			});
		} else {
			branch.final_recovery_head = Some(owner.clone());
		}
		branch.final_recovery_tail = Some(owner.clone());
		Ok(())
	})?;

	Pallet::<T>::deposit_event(Event::FinalRecoveryEntered {
		collateral_id: *collateral_id,
		owner,
	});
	Ok(())
}

/// Remove `owner` from the per-branch FIFO. Errors if not present.
pub fn remove<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Result<(), DispatchError> {
	let node = FinalRecoveryNodes::<T>::take(collateral_id, owner)
		.ok_or(Error::<T>::FinalRecoveryInvariantBroken)?;
	BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
		let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		match (&node.prev, &node.next) {
			(Some(p), Some(n)) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, p, |maybe| {
					if let Some(left) = maybe {
						left.next = Some(n.clone());
					}
				});
				FinalRecoveryNodes::<T>::mutate(collateral_id, n, |maybe| {
					if let Some(right) = maybe {
						right.prev = Some(p.clone());
					}
				});
			},
			(Some(p), None) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, p, |maybe| {
					if let Some(left) = maybe {
						left.next = None;
					}
				});
				branch.final_recovery_tail = Some(p.clone());
			},
			(None, Some(n)) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, n, |maybe| {
					if let Some(right) = maybe {
						right.prev = None;
					}
				});
				branch.final_recovery_head = Some(n.clone());
			},
			(None, None) => {
				branch.final_recovery_head = None;
				branch.final_recovery_tail = None;
			},
		}
		Ok(())
	})?;
	Pallet::<T>::deposit_event(Event::FinalRecoveryExited {
		collateral_id: *collateral_id,
		owner: owner.clone(),
	});
	Ok(())
}

/// Peek the head of the FIFO, if any.
pub fn next_target<T: Config>(collateral_id: &T::AssetId) -> Option<T::AccountId> {
	BranchStates::<T>::get(collateral_id).and_then(|s| s.final_recovery_head)
}

/// First `n` FIFO owners, head-first.
pub fn queue_head<T: Config>(collateral_id: &T::AssetId, n: u32) -> Vec<T::AccountId> {
	let mut out = Vec::with_capacity(n as usize);
	let mut cursor = next_target::<T>(collateral_id);
	let mut taken = 0u32;
	while let Some(owner) = cursor {
		if taken >= n {
			break;
		}
		let node = match FinalRecoveryNodes::<T>::get(collateral_id, &owner) {
			Some(node) => node,
			None => break,
		};
		out.push(owner);
		cursor = node.next;
		taken = taken.saturating_add(1);
	}
	out
}

/// Recovery settlement terms for a vault that's currently in `FinalRecovery`.
///
/// Computed from the vault's fully-accrued state at the current oracle price.
/// The caller is expected to have already touched the vault before calling.
#[allow(dead_code)]
pub struct RecoveryTerms<Balance> {
	pub pricing: RecoveryPricing,
	/// Collateral the redeemer receives per pUSD burnt at the current oracle
	/// price: in the bonus regime this is `(1 + bonus) / price`; in the
	/// insurance-adjusted regime it is `recovery_rate / price`.
	pub effective_per_pusd: FixedU128,
	/// Maximum debt that can be cancelled in this settlement step.
	pub max_cancellable_debt: Balance,
	/// pUSD that the Insurance Fund must burn for residual bad debt — only
	/// non-zero in the `InsuranceAdjustedBelowOneHundred` regime.
	pub effective_cover: Balance,
}

/// Compute the settlement terms given the vault's accrued debt, current
/// collateral, oracle price, redistribution penalty cap, and Insurance Fund
/// balance.
///
/// Returns `None` if the price is zero (callers should treat that as
/// `RecoveryPricingUnavailable`).
#[allow(dead_code)]
pub fn settlement_terms<Balance>(
	debt: Balance,
	collateral: Balance,
	price: FixedU128,
	redistribution_penalty: FixedU128,
	insurance_fund_balance: Balance,
) -> Option<RecoveryTerms<Balance>>
where
	Balance: Copy + Zero + Saturating + Ord + From<u128> + Into<u128>,
{
	if price.is_zero() {
		return None;
	}
	let d: u128 = debt.into();
	let c: u128 = collateral.into();
	if d == 0 {
		return Some(RecoveryTerms {
			pricing: RecoveryPricing::BonusAboveOneHundred,
			effective_per_pusd: FixedU128::zero(),
			max_cancellable_debt: Balance::zero(),
			effective_cover: Balance::zero(),
		});
	}

	let collateral_value = price.saturating_mul(FixedU128::saturating_from_integer(c));
	// `d > 0` is asserted at L193, so `denom_d` is non-zero. We still go through
	// `checked_div` rather than `/` to avoid the panicking-on-overflow branch.
	let denom_d = FixedU128::saturating_from_integer(d);
	let cr = collateral_value.checked_div(&denom_d)?;
	let one = FixedU128::saturating_from_integer(1u128);

	if cr >= one {
		let raw_bonus = cr.saturating_sub(one);
		let bonus = raw_bonus.min(redistribution_penalty);
		let multiplier = one.saturating_add(bonus);
		// `price > 0` is asserted at L188-190, so this `checked_div` only
		// returns `None` on overflow — propagate as `None` then.
		let effective_per_pusd = multiplier.checked_div(&price)?;
		Some(RecoveryTerms {
			pricing: RecoveryPricing::BonusAboveOneHundred,
			effective_per_pusd,
			max_cancellable_debt: debt,
			effective_cover: Balance::zero(),
		})
	} else {
		let shortfall = d.saturating_sub(c);
		let cover = core::cmp::min::<u128>(insurance_fund_balance.into(), shortfall);
		let market_cancel_debt = d.saturating_sub(cover);
		if market_cancel_debt == 0 {
			return Some(RecoveryTerms {
				pricing: RecoveryPricing::InsuranceAdjustedBelowOneHundred,
				effective_per_pusd: FixedU128::zero(),
				max_cancellable_debt: Balance::zero(),
				effective_cover: Balance::from(cover),
			});
		}
		let recovery_rate = if c == 0 {
			FixedU128::zero()
		} else {
			// `market_cancel_debt > 0` per the early-return above.
			FixedU128::saturating_from_integer(c).checked_div(
				&FixedU128::saturating_from_integer(market_cancel_debt),
			)?
		};
		let effective_per_pusd = recovery_rate.checked_div(&price)?;
		Some(RecoveryTerms {
			pricing: RecoveryPricing::InsuranceAdjustedBelowOneHundred,
			effective_per_pusd,
			max_cancellable_debt: Balance::from(market_cancel_debt),
			effective_cover: Balance::from(cover),
		})
	}
}
