//! Liquidation handoff types and trait

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame::deps::frame_support::pallet_prelude::{DispatchError, DispatchResult};
use scale_info::TypeInfo;

/// Debt cancelled by external pUSD (Stability Pool + JIT combined) and the
/// matching collateral credited to the offset path. The orchestrator may
/// internally split the collateral across standing depositors and JIT.
///
/// `recipient` is the account that receives `collateral` — the vault pallet
/// moves it inline during `finalize_liquidation`, so the orchestrator never
/// needs to take possession of liquidated collateral itself.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct OffsetAllocation<AccountId, Balance> {
	pub recipient: AccountId,
	pub debt: Balance,
	pub collateral: Balance,
}

/// Compensation paid to the liquidation keeper.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct KeeperCompensation<AccountId, Balance> {
	pub recipient: AccountId,
	pub collateral: Balance,
}

/// Allocation produced by the liquidation orchestrator and applied by
/// [`VaultLiquidationInterface::finalize_liquidation`].
///
/// Redistributed debt is derived inside `finalize_liquidation` as
/// `post_touch_debt - offset.debt`, so the orchestrator only needs to specify
/// the collateral split.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct LiquidationAllocation<AccountId, Balance> {
	pub offset: OffsetAllocation<AccountId, Balance>,
	pub redistribution_collateral: Balance,
	pub keeper: KeeperCompensation<AccountId, Balance>,
}

/// Two-call liquidation hook. The vault pallet implements both calls; the
/// orchestrator (`pallet-liquidation` or equivalent) drives them in a single
/// dispatch.
pub trait VaultLiquidationInterface<AccountId, AssetId, Balance> {
	/// Update aggregate interest, touch the vault, apply pending
	/// redistribution, remove it from the rate index, subtract its post-touch
	/// contributions from branch aggregates, and return the post-touch debt
	/// the orchestrator must settle.
	fn prepare_liquidation(
		collateral_id: AssetId,
		owner: AccountId,
	) -> Result<Balance, DispatchError>;

	/// Re-read the post-touch debt and held collateral, validate the
	/// allocation against current state, advance redistribution accumulators,
	/// pay offset/keeper/owner-surplus, and remove the vault row.
	fn finalize_liquidation(
		collateral_id: AssetId,
		owner: AccountId,
		allocation: LiquidationAllocation<AccountId, Balance>,
	) -> DispatchResult;
}
