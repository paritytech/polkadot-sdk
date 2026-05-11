//! Storage and value types for `pallet-vaults`.
//!
//! See `troves.md` §5 for the canonical reference.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::sp_runtime::{traits::Zero, FixedU128};
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

/// Per-vault state. The vault's collateral lives on the `VaultCollateral`
/// hold for `(owner, collateral_id)` and is intentionally NOT stored here.
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
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct VaultRedistSnapshot {
	pub collat_per_stake: FixedU128,
	pub debt_per_stake: FixedU128,
	pub debt_time_per_stake: FixedU128,
}

impl Default for VaultRedistSnapshot {
	fn default() -> Self {
		Self {
			collat_per_stake: FixedU128::zero(),
			debt_per_stake: FixedU128::zero(),
			debt_time_per_stake: FixedU128::zero(),
		}
	}
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

/// Cold redistribution accumulators per branch, stored separately so that
/// ordinary interest-only touches don't rewrite them.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchRedistState<Balance> {
	pub total_collateral_snapshot: Balance,
	pub total_stakes_snapshot: Balance,
	pub cumulative_redist_collat_per_stake: FixedU128,
	pub cumulative_redist_debt_per_stake: FixedU128,
	pub cumulative_redist_debt_time_per_stake: FixedU128,
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
