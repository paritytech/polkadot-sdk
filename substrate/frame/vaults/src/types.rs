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

/// Logical linked-list partitions owned by this pallet.
///
/// `Rate(asset)` is the active-vault rate index. `FinalRecovery(asset)` is
/// the per-branch recovery FIFO, using a monotonically increasing insertion
/// sequence as the stored priority.
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
	PartialOrd,
	Ord,
	Debug,
)]
pub enum VaultListId<AssetId> {
	Rate(AssetId),
	FinalRecovery(AssetId),
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

/// Debt cancelled from a vault, split by bucket.
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
	Default,
)]
pub struct DebtPayment<Balance> {
	pub interest: Balance,
	pub principal: Balance,
}

impl<Balance> DebtPayment<Balance>
where
	Balance: Saturating + Copy,
{
	pub fn total(&self) -> Balance {
		self.interest.saturating_add(self.principal)
	}
}

/// Debt tracked on a vault row.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct VaultDebt<Balance> {
	pub principal: Balance,
	pub interest: Balance,
}

impl<Balance> VaultDebt<Balance>
where
	Balance: Ord + Saturating + Copy,
{
	pub fn total(&self) -> Balance {
		self.principal.saturating_add(self.interest)
	}

	pub fn cancel(&mut self, amount: Balance) -> DebtPayment<Balance> {
		let interest = core::cmp::min(amount, self.interest);
		self.interest = self.interest.saturating_sub(interest);
		let remaining = amount.saturating_sub(interest);
		let principal = core::cmp::min(remaining, self.principal);
		self.principal = self.principal.saturating_sub(principal);
		DebtPayment { interest, principal }
	}
}

/// Snapshot of branch redistribution accumulators stamped at vault open and
/// re-stamped whenever pending redistribution is applied.
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
	Default,
)]
pub struct RedistSnapshot {
	pub collat_per_stake: FixedU128,
	pub debt_per_stake: FixedU128,
	pub debt_time_per_stake: FixedU128,
	pub weight_per_stake: FixedU128,
}

/// Per-vault state. The vault's collateral lives on the `VaultCollateral`
/// hold for `(owner, collateral_id)` and is intentionally NOT stored here.
/// `redistribution_stake` is the at-open snapshot of the vault's
/// redistribution share: deposits/withdrawals do not change it, matching the
/// branch's frozen stake accounting. Reads of "current collateral" go through
/// `held_collateral(...)`, not `redistribution_stake`.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct Vault<Balance, Moment> {
	pub debt: VaultDebt<Balance>,
	pub annual_rate: FixedU128,
	pub last_interest_update: Moment,
	pub last_rate_update: Moment,
	pub redistribution_stake: Balance,
	pub redist_snapshot: RedistSnapshot,
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

/// Debt and interest aggregates for one collateral branch.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchDebt<Balance, Moment> {
	pub principal: Balance,
	pub minted_interest: Balance,
	pub pending_redist_principal: Balance,
	pub bad_debt: Balance,
	pub weighted_principal_sum: Balance,
	pub last_interest_update: Moment,
}

/// Frozen redistribution stake totals for one collateral branch.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchStakes<Balance> {
	pub total: Balance,
	pub weighted_sum: Balance,
}

/// Queue/cursor state used to derive non-debt vault status.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchQueues<AccountId> {
	pub next_final_recovery_nonce: u128,
	pub last_dormant_vault_owner: Option<AccountId>,
	pub idle_cursor: Option<AccountId>,
}

/// Per-branch accounting state.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchState<AccountId, Balance, Moment> {
	pub total_collateral: Balance,
	pub debt: BranchDebt<Balance, Moment>,
	pub stakes: BranchStakes<Balance>,
	pub redist: RedistSnapshot,
	pub queues: BranchQueues<AccountId>,
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
	/// Add a vault's full contribution to branch debt/stake aggregates.
	pub fn attach_vault(&mut self, vault: &Vault<Balance, Moment>) {
		let rate_x_debt = vault.annual_rate.saturating_mul_int(vault.debt.principal);
		let rate_x_stake = vault.annual_rate.saturating_mul_int(vault.redistribution_stake);
		self.debt.principal = self.debt.principal.saturating_add(vault.debt.principal);
		self.debt.minted_interest = self.debt.minted_interest.saturating_add(vault.debt.interest);
		self.debt.weighted_principal_sum =
			self.debt.weighted_principal_sum.saturating_add(rate_x_debt);
		self.stakes.weighted_sum = self.stakes.weighted_sum.saturating_add(rate_x_stake);
		self.stakes.total = self.stakes.total.saturating_add(vault.redistribution_stake);
	}

	/// Subtract a vault's full contribution from the branch aggregates.
	///
	/// Mirrors the addition done at vault open: every writer that mutates
	/// `(debt.principal, debt.interest, redistribution_stake)` for a vault must
	/// keep this sum-of-contributions invariant intact, so removal is the
	/// exact inverse — recompute the same `(rate * debt, rate * stake)`
	/// products and subtract.
	pub fn detach_vault(&mut self, vault: &Vault<Balance, Moment>) {
		let rate_x_debt = vault.annual_rate.saturating_mul_int(vault.debt.principal);
		let rate_x_stake = vault.annual_rate.saturating_mul_int(vault.redistribution_stake);
		self.debt.principal = self.debt.principal.saturating_sub(vault.debt.principal);
		self.debt.minted_interest = self.debt.minted_interest.saturating_sub(vault.debt.interest);
		self.debt.weighted_principal_sum =
			self.debt.weighted_principal_sum.saturating_sub(rate_x_debt);
		self.stakes.weighted_sum = self.stakes.weighted_sum.saturating_sub(rate_x_stake);
		self.stakes.total = self.stakes.total.saturating_sub(vault.redistribution_stake);
	}

	pub fn add_collateral(&mut self, amount: Balance) {
		self.total_collateral = self.total_collateral.saturating_add(amount);
	}

	pub fn remove_collateral(&mut self, amount: Balance) {
		self.total_collateral = self.total_collateral.saturating_sub(amount);
	}

	pub fn apply_debt_payment(&mut self, payment: DebtPayment<Balance>, rate: FixedU128) {
		self.debt.principal = self.debt.principal.saturating_sub(payment.principal);
		self.debt.minted_interest = self.debt.minted_interest.saturating_sub(payment.interest);
		let weighted = rate.saturating_mul_int(payment.principal);
		self.debt.weighted_principal_sum =
			self.debt.weighted_principal_sum.saturating_sub(weighted);
	}

	pub fn change_vault_rate(
		&mut self,
		old_rate: FixedU128,
		new_rate: FixedU128,
		principal: Balance,
		stake: Balance,
	) {
		self.debt.weighted_principal_sum = self
			.debt
			.weighted_principal_sum
			.saturating_sub(old_rate.saturating_mul_int(principal))
			.saturating_add(new_rate.saturating_mul_int(principal));
		self.stakes.weighted_sum = self
			.stakes
			.weighted_sum
			.saturating_sub(old_rate.saturating_mul_int(stake))
			.saturating_add(new_rate.saturating_mul_int(stake));
	}
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
