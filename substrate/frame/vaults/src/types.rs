//! Storage and value types for `pallet-vaults`.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::sp_runtime::{
	traits::Saturating, FixedPointNumber, FixedPointOperand, FixedU128, Permill,
};
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

impl<AssetId: Default> Default for VaultListId<AssetId> {
	fn default() -> Self {
		Self::Rate(AssetId::default())
	}
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

impl<Balance: Saturating + Copy> DebtPayment<Balance> {
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

impl<Balance: Ord + Saturating + Copy> VaultDebt<Balance> {
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
/// `redistribution_stake` mirrors the vault's *current* eligible collateral:
/// it must equal the `VaultCollateral` hold while the vault is `Active` or
/// `Dormant` and is set to zero while the vault is in `FinalRecovery`. The
/// field is refreshed after every op that changes collateral or eligibility,
/// always after pending redistribution has been applied, so
/// `BranchStakes.total == Σ vault.redistribution_stake` over the live
/// eligible set.
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
	pub minimum_borrow_rate: FixedU128,
	pub maximum_borrow_rate: FixedU128,
	pub upfront_fee_period: Moment,
	pub rate_adjustment_cooldown: Moment,
	pub redistribution_penalty: Permill,
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

/// Current-collateral redistribution stake totals for one collateral branch.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchStakes<Balance> {
	pub total: Balance,
	pub weighted_sum: Balance,
}

/// Per-branch ownerless rounding residue.
///
/// `ownerless_pusd_debt` is debt that exists at the branch level but cannot
/// be attributed to any specific vault (typically per-stake flooring residue
/// from a redistribution). `ownerless_pusd_surplus` is the mirror image:
/// pUSD that arrived without an owner. `add_ownerless_pusd_*` netting keeps
/// `surplus * debt == 0` so the surplus offsets debt as soon as it appears.
/// `ownerless_collateral_surplus` is collateral that sits on the
/// redistribution account but cannot be attributed; it is bookkeeping only,
/// since the physical balance is already held there.
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
pub struct BranchRounding<Balance> {
	pub ownerless_pusd_debt: Balance,
	pub ownerless_pusd_surplus: Balance,
	pub ownerless_collateral_surplus: Balance,
}

/// Per-branch accounting state.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct BranchState<AccountId, Balance, Moment> {
	pub total_collateral: Balance,
	pub debt: BranchDebt<Balance, Moment>,
	pub stakes: BranchStakes<Balance>,
	pub rounding: BranchRounding<Balance>,
	pub redist: RedistSnapshot,
	pub epoch: Moment,
	pub next_final_recovery_nonce: u128,
	pub last_dormant_vault_owner: Option<AccountId>,
	pub idle_cursor: Option<AccountId>,
	pub frozen: Option<FrozenState<Moment>>,
}

impl<AccountId, Balance, Moment> BranchState<AccountId, Balance, Moment> {
	pub fn is_frozen(&self) -> bool {
		self.frozen.is_some()
	}
}

impl<AccountId, Balance: FixedPointOperand + Saturating, Moment>
	BranchState<AccountId, Balance, Moment>
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

	/// Apply a debt payment to the branch aggregates. `principal_after` is the
	/// paying vault's principal *after* `VaultDebt::cancel` ran.
	pub fn apply_debt_payment(
		&mut self,
		payment: DebtPayment<Balance>,
		rate: FixedU128,
		principal_after: Balance,
	) {
		self.debt.principal = self.debt.principal.saturating_sub(payment.principal);
		self.debt.minted_interest = self.debt.minted_interest.saturating_sub(payment.interest);
		let principal_before = principal_after.saturating_add(payment.principal);
		self.debt.weighted_principal_sum = self
			.debt
			.weighted_principal_sum
			.saturating_sub(rate.saturating_mul_int(principal_before))
			.saturating_add(rate.saturating_mul_int(principal_after));
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

	/// Swap a vault's stake contribution after collateral or eligibility has
	/// changed. The vault rate is unchanged here; rate moves go through
	/// [`Self::change_vault_rate`].
	pub fn refresh_vault_stake(&mut self, rate: FixedU128, old_stake: Balance, new_stake: Balance) {
		self.stakes.total = self.stakes.total.saturating_sub(old_stake).saturating_add(new_stake);
		self.stakes.weighted_sum = self
			.stakes
			.weighted_sum
			.saturating_sub(rate.saturating_mul_int(old_stake))
			.saturating_add(rate.saturating_mul_int(new_stake));
	}
}

impl<AccountId, Balance: Ord + Saturating + Copy, Moment> BranchState<AccountId, Balance, Moment> {
	/// Deposit ownerless pUSD debt, netting against any existing ownerless
	/// surplus first. Preserves the invariant `surplus * debt == 0`.
	pub fn add_ownerless_pusd_debt(&mut self, amount: Balance) {
		let offset = core::cmp::min(amount, self.rounding.ownerless_pusd_surplus);
		self.rounding.ownerless_pusd_surplus =
			self.rounding.ownerless_pusd_surplus.saturating_sub(offset);
		self.rounding.ownerless_pusd_debt =
			self.rounding.ownerless_pusd_debt.saturating_add(amount.saturating_sub(offset));
	}

	/// Deposit ownerless pUSD surplus, netting against any existing ownerless
	/// debt first. Preserves the invariant `surplus * debt == 0`.
	pub fn add_ownerless_pusd_surplus(&mut self, amount: Balance) {
		let offset = core::cmp::min(amount, self.rounding.ownerless_pusd_debt);
		self.rounding.ownerless_pusd_debt =
			self.rounding.ownerless_pusd_debt.saturating_sub(offset);
		self.rounding.ownerless_pusd_surplus = self
			.rounding
			.ownerless_pusd_surplus
			.saturating_add(amount.saturating_sub(offset));
	}

	pub fn add_ownerless_collateral_surplus(&mut self, amount: Balance) {
		self.rounding.ownerless_collateral_surplus =
			self.rounding.ownerless_collateral_surplus.saturating_add(amount);
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
	DebtCeiling,
	MinimumDebt,
	MinimumCollateral,
	BorrowRateBounds,
	UpfrontFeePeriod,
	RateAdjustmentCooldown,
	RedistributionPenalty,
}

/// Atomic update to a single field of `BranchConfig`. Carries both the
/// `ParameterId` tag (for event emission) and the new value, so the two can't
/// drift.
pub enum BranchConfigUpdate<Balance, Moment> {
	MinimumCollateralizationRatio(FixedU128),
	InitialCollateralizationRatio(FixedU128),
	SafetyCollateralizationRatio(FixedU128),
	DebtCeiling(Balance),
	MinimumDebt(Balance),
	MinimumCollateral(Balance),
	BorrowRateBounds { min: FixedU128, max: FixedU128 },
	UpfrontFeePeriod(Moment),
	RateAdjustmentCooldown(Moment),
	RedistributionPenalty(Permill),
}

impl<Balance, Moment> BranchConfigUpdate<Balance, Moment> {
	pub fn parameter_id(&self) -> ParameterId {
		match self {
			Self::MinimumCollateralizationRatio(_) => ParameterId::MinimumCollateralizationRatio,
			Self::InitialCollateralizationRatio(_) => ParameterId::InitialCollateralizationRatio,
			Self::SafetyCollateralizationRatio(_) => ParameterId::SafetyCollateralizationRatio,
			Self::DebtCeiling(_) => ParameterId::DebtCeiling,
			Self::MinimumDebt(_) => ParameterId::MinimumDebt,
			Self::MinimumCollateral(_) => ParameterId::MinimumCollateral,
			Self::BorrowRateBounds { .. } => ParameterId::BorrowRateBounds,
			Self::UpfrontFeePeriod(_) => ParameterId::UpfrontFeePeriod,
			Self::RateAdjustmentCooldown(_) => ParameterId::RateAdjustmentCooldown,
			Self::RedistributionPenalty(_) => ParameterId::RedistributionPenalty,
		}
	}

	pub fn apply_to(self, cfg: &mut BranchConfig<Balance, Moment>) {
		match self {
			Self::MinimumCollateralizationRatio(v) => cfg.minimum_collateralization_ratio = v,
			Self::InitialCollateralizationRatio(v) => cfg.initial_collateralization_ratio = v,
			Self::SafetyCollateralizationRatio(v) => cfg.safety_collateralization_ratio = v,
			Self::DebtCeiling(v) => cfg.debt_ceiling = v,
			Self::MinimumDebt(v) => cfg.minimum_debt = v,
			Self::MinimumCollateral(v) => cfg.minimum_collateral = v,
			Self::BorrowRateBounds { min, max } => {
				cfg.minimum_borrow_rate = min;
				cfg.maximum_borrow_rate = max;
			},
			Self::UpfrontFeePeriod(v) => cfg.upfront_fee_period = v,
			Self::RateAdjustmentCooldown(v) => cfg.rate_adjustment_cooldown = v,
			Self::RedistributionPenalty(v) => cfg.redistribution_penalty = v,
		}
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	fn branch_state(principal: u128, weighted: u128) -> BranchState<u64, u128, u64> {
		BranchState {
			total_collateral: 0,
			debt: BranchDebt {
				principal,
				minted_interest: 0,
				pending_redist_principal: 0,
				bad_debt: 0,
				weighted_principal_sum: weighted,
				last_interest_update: 0,
			},
			stakes: BranchStakes { total: 0, weighted_sum: 0 },
			rounding: BranchRounding::default(),
			redist: RedistSnapshot::default(),
			epoch: 0,
			next_final_recovery_nonce: 0,
			last_dormant_vault_owner: None,
			idle_cursor: None,
			frozen: None,
		}
	}

	#[test]
	fn apply_debt_payment_swaps_full_contribution() {
		// rate = 0.3: floor(0.3 * 10) = 3 and floor(0.3 * 9) = 2. The naive
		// `floor(rate * delta)` update would subtract floor(0.3 * 1) = 0 and
		// strand the weighted sum at 3.
		let rate = FixedU128::from_rational(3u128, 10u128);
		let mut bs = branch_state(10, 3);
		bs.apply_debt_payment(DebtPayment { interest: 0, principal: 1 }, rate, 9);
		assert_eq!(bs.debt.principal, 9);
		assert_eq!(bs.debt.weighted_principal_sum, 2);
	}

	#[test]
	fn apply_debt_payment_full_payoff_clears_contribution() {
		let rate = FixedU128::from_rational(3u128, 10u128);
		let mut bs = branch_state(10, 3);
		bs.apply_debt_payment(DebtPayment { interest: 0, principal: 10 }, rate, 0);
		assert_eq!(bs.debt.principal, 0);
		assert_eq!(bs.debt.weighted_principal_sum, 0);
	}
}
