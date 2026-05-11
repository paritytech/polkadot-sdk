//! # pUSD Primitives
//!
//! Shared types and traits for the pUSD protocol pallets (vaults, redemptions,
//! liquidation, stability pool, ...). Carries no pallet-specific assumptions:
//! every type is parameterised over the consumer's `AccountId`, `AssetId`,
//! `Balance`, and credit/debt imbalance shapes.
//!
//! See `liquity_v2/polkadot-impl/troves.md` for the design document this crate
//! is extracted from.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame::deps::sp_runtime::FixedU128;

pub mod bad_debt;
pub mod branch;
pub mod liquidation;
pub mod oracle;
pub mod redemption;
pub mod yield_sink;

pub use bad_debt::VaultBadDebtInterface;
pub use branch::{BranchMode, BranchModeProvider, FrozenReason, FrozenState};
pub use liquidation::{
	KeeperCompensation, LiquidationAllocation, OffsetAllocation, VaultLiquidationInterface,
};
pub use oracle::{PriceFeed, ProvidePrice};
pub use redemption::{RedemptionAllocation, VaultRedemptionInterface};
pub use yield_sink::OnBranchYield;

/// Number of milliseconds in one calendar year, matching `troves.md` §3.
pub const MILLIS_PER_YEAR: u64 = 31_557_600_000;

/// Convenience alias for the rate type used by the rate-ordered redemption
/// index. `FixedU128` matches the `pallet-linked-list` `Score` type configured
/// by `pallet-vaults`.
pub type AnnualRate = FixedU128;
