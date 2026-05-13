//! Storage and value types for `pallet-vaults`.
//!
//! See `troves.md` §5 for the canonical reference.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::sp_runtime::{traits::Saturating, FixedPointNumber, FixedPointOperand, FixedU128};
use scale_info::TypeInfo;

pub use pusd_primitives::{BranchMode, FrozenReason, FrozenState};

/// Lifecycle status of a vault.
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
pub enum VaultStatus {
	/// Debt-bearing vault with `Debt >= MinimumDebt`. In the rate index.
	Active,
	/// Below `MinimumDebt` (possibly zero) after redemption. Out of the rate
	/// index, may be revived to `Active`.
	Dormant,
	/// Below MCR last-eligible vault parked in the FIFO and resolved by
	/// recovery redemptions / offsets.
	FinalRecovery,
}

impl VaultStatus {
	/// Debt-bearing vault, present in the rate index.
	pub fn is_active(&self) -> bool {
		matches!(self, Self::Active)
	}

	/// Drained below `minimum_debt`, out of the rate index.
	pub fn is_dormant(&self) -> bool {
		matches!(self, Self::Dormant)
	}

	/// Parked in the FIFO awaiting recovery settlement.
	pub fn is_final_recovery(&self) -> bool {
		matches!(self, Self::FinalRecovery)
	}
}

/// Per-vault state. The vault's collateral lives on the `VaultCollateral`
/// hold for `(owner, collateral_id)` and is intentionally NOT stored here.
/// `stake` is the at-open snapshot of the vault's redistribution share — it
/// is frozen for the vault's lifetime (deposits/withdrawals don't change it),
/// matching `bs.total_stakes` accounting. Reads of "current collateral" go
/// through `held_collateral(...)` not `vault.stake`.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct Vault<Balance, Moment> {
	pub status: VaultStatus,
	pub interest_bearing_debt: Balance,
	pub accrued_interest: Balance,
	pub annual_rate: FixedU128,
	pub last_interest_update: Moment,
	pub last_rate_update: Moment,
	pub stake: Balance,
	pub redist_epoch: u64,
}

/// Snapshot of branch redistribution accumulators stamped at vault open and
/// re-stamped on each touch that crosses a redistribution epoch boundary.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	Debug,
	Default,
)]
pub struct VaultRedistSnapshot {
	pub collat_per_stake: FixedU128,
	pub debt_per_stake: FixedU128,
	pub debt_time_per_stake: FixedU128,
	/// Snapshot of the branch's cumulative avg-rate weighted contribution per
	/// stake. Used on touch to reconcile the recipient's share of the
	/// avg-rate interest-base fold with the recipient's own rate.
	pub weight_per_stake: FixedU128,
}

/// Branch governance/risk parameters.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchConfig<Balance, Moment> {
	pub minimum_collateralization_ratio: FixedU128,
	pub initial_collateralization_ratio: FixedU128,
	pub safety_collateralization_ratio: FixedU128,
	pub debt_ceiling: Balance,
	pub minimum_debt: Balance,
	pub minimum_collateral: Balance,
	pub minimum_total_stakes: Balance,
	pub minimum_borrow_rate: FixedU128,
	pub maximum_borrow_rate: FixedU128,
	pub upfront_fee_period: Moment,
	pub rate_adjustment_cooldown: Moment,
	pub redistribution_penalty: FixedU128,
}

/// Hot per-branch accounting state.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchState<AccountId, Balance, Moment> {
	pub total_collateral: Balance,
	pub total_interest_bearing_debt: Balance,
	pub total_minted_aggregate_interest: Balance,
	pub pending_redistribution_debt: Balance,
	pub bad_debt: Balance,
	pub weighted_interest_bearing_debt_sum: Balance,
	pub last_aggregate_interest_update: Moment,
	pub total_stakes: Balance,
	pub weighted_stake_sum: Balance,
	pub redist_epoch: u64,
	pub final_recovery_head: Option<AccountId>,
	pub final_recovery_tail: Option<AccountId>,
	pub last_dormant_vault_owner: Option<AccountId>,
	pub frozen: Option<FrozenState<Moment>>,
}

impl<AccountId, Balance, Moment> BranchState<AccountId, Balance, Moment> {
	pub fn is_frozen(&self) -> bool {
		self.frozen.is_some()
	}
}

impl<AccountId, Balance, Moment> BranchState<AccountId, Balance, Moment>
where
	Balance: FixedPointOperand + Saturating,
{
	/// Subtract a vault's full contribution from the branch aggregates.
	///
	/// Mirrors the addition done at vault open: every writer that mutates
	/// `(interest_bearing_debt, accrued_interest, stake)` for a vault must
	/// keep this sum-of-contributions invariant intact, so removal is the
	/// exact inverse — recompute the same `(rate * debt, rate * stake)`
	/// products and subtract.
	pub fn detach_vault(&mut self, vault: &Vault<Balance, Moment>) {
		let rate_x_debt = vault.annual_rate.saturating_mul_int(vault.interest_bearing_debt);
		let rate_x_stake = vault.annual_rate.saturating_mul_int(vault.stake);
		self.total_interest_bearing_debt =
			self.total_interest_bearing_debt.saturating_sub(vault.interest_bearing_debt);
		self.total_minted_aggregate_interest =
			self.total_minted_aggregate_interest.saturating_sub(vault.accrued_interest);
		self.weighted_interest_bearing_debt_sum =
			self.weighted_interest_bearing_debt_sum.saturating_sub(rate_x_debt);
		self.weighted_stake_sum = self.weighted_stake_sum.saturating_sub(rate_x_stake);
		self.total_stakes = self.total_stakes.saturating_sub(vault.stake);
	}
}

/// Cold redistribution accumulators per branch, stored separately so that
/// ordinary interest-only touches don't rewrite them.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchRedistState {
	pub cumulative_redist_collat_per_stake: FixedU128,
	pub cumulative_redist_debt_per_stake: FixedU128,
	pub cumulative_redist_debt_time_per_stake: FixedU128,
	/// Cumulative per-stake share of the avg-rate weighted contribution folded
	/// into `weighted_interest_bearing_debt_sum` at liquidation. On vault
	/// touch, `stake * (cumulative - snapshot)` is subtracted from
	/// `weighted_interest_bearing_debt_sum` and the recipient's own-rate share
	/// is added back. Tracks the "avg-rate at liquidation" delta that the
	/// per-vault reconciliation needs to undo.
	pub cumulative_redist_weight_per_stake: FixedU128,
}

/// FIFO node for the per-branch `FinalRecovery` queue.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct FinalRecoveryNode<AccountId, Moment> {
	pub prev: Option<AccountId>,
	pub next: Option<AccountId>,
	pub entered_at: Moment,
}

/// Identifier for the parameter changed by an `Event::ParameterUpdated`
/// emission. Lets indexers filter governance changes without consulting the
/// extrinsic call data.
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
pub enum ParameterId {
	MinimumCollateralizationRatio,
	InitialCollateralizationRatio,
	SafetyCollateralizationRatio,
	MinimumDebt,
	MinimumCollateral,
	MinimumTotalStakes,
	BorrowRateBounds,
	UpfrontFeePeriod,
	RateAdjustmentCooldown,
	RedistributionPenalty,
}

/// Manager-origin authorization tier.
///
/// `Full` may register branches and update any parameter. `Defensive` may only
/// take risk-reducing actions: lower debt ceiling, raise collateralization
/// thresholds, force `Frozen` mode, or reduce max borrow rate.
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
pub enum VaultsManagerLevel {
	Full,
	Defensive,
}
