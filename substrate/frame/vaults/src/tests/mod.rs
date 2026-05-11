//! `pallet-vaults` test suite.
//!
//! Organized to mirror the structure of `liquity_v2/polkadot-impl/tests.md`,
//! which catalogues 142 spec rows that should be exercised against the
//! Polkadot port. Each child module corresponds to one source-of-truth file
//! in the Liquity V2 Solidity test suite (Group A/B/D/F per `tests.md`).
//!
//! Mock harness lives in `crate::mock`. Tests use `build_and_execute(...)`
//! which runs `try_state` invariants post-test when the `try-runtime`
//! feature is enabled.

mod basic_ops;
mod borrower_operations;
mod critical_threshold;
mod debt_in_front;
mod events;
mod hint_helpers;
mod interest_rate;
mod last_vault;
mod lifecycle;
mod redemptions;
mod sorted_troves;

use frame::deps::sp_runtime::FixedU128;

/// Convenience: build a `FixedU128` rate from a `num/denom` ratio.
pub(super) fn rate_pct(num: u128, denom: u128) -> FixedU128 {
	FixedU128::from_rational(num, denom)
}
