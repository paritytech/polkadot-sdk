//! Redemption handoff types and trait.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::frame_support::pallet_prelude::{DispatchError, DispatchResult};
use scale_info::TypeInfo;

/// Per-vault allocation produced by the redemption orchestrator and applied by
/// [`VaultRedemptionInterface::apply_redemption`].
///
/// `fee_collateral_retained` stays in the vault as a branch-local fee.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct RedemptionAllocation<Balance> {
	pub debt_to_cancel: Balance,
	pub collateral_to_redeemer: Balance,
	pub fee_collateral_retained: Balance,
}

/// Three-call redemption hook. The orchestrator repeatedly reads
/// `next_redemption_target`, touches that candidate, sizes the allocation
/// against the post-touch debt and the held collateral, then applies it; each
/// applied redemption re-shapes the priority queue, so the next read returns the
/// new head.
pub trait VaultRedemptionInterface<AccountId, AssetId, Balance> {
	/// Highest-priority vault owner to redeem against on `collateral_id`, or
	/// `None` when the branch has no redeemable vaults.
	///
	/// The vault pallet's authoritative redemption order is: `FinalRecovery`
	/// FIFO first, then `last_dormant_vault_owner`, then the rate index
	/// tail-first. Redemption always targets the current head.
	fn next_redemption_target(collateral_id: AssetId) -> Option<AccountId>;

	/// Touch the vault, apply pending interest and redistribution, and return
	/// the post-touch debt the orchestrator must cap `debt_to_cancel` against.
	fn touch_for_redemption(
		collateral_id: AssetId,
		owner: AccountId,
	) -> Result<Balance, DispatchError>;

	/// Apply the orchestrator's per-vault allocation. The vault re-reads the
	/// post-touch debt and held collateral, verifies conservation, transitions
	/// to `Dormant` if the residual debt is below `MinimumDebt`, and updates
	/// `last_dormant_vault_owner` accordingly.
	fn apply_redemption(
		collateral_id: AssetId,
		owner: AccountId,
		redeemer: AccountId,
		allocation: RedemptionAllocation<Balance>,
	) -> DispatchResult;
}
