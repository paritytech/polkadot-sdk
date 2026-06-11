//! Bad-debt recording and healing trait.

use frame::deps::{frame_support::pallet_prelude::DispatchResult, sp_runtime::DispatchError};

/// Branch-level bad-debt accounting surface.
///
/// `record_bad_debt` is the increment side, called by the orchestrator when a
/// liquidation cannot cover a vault's debt. `heal` is the inverse: the
/// orchestrator withdraws from the
/// Insurance Fund as a `fungible::Credit` and hands it here; the
/// implementation rescinds the underlying pUSD and decrements the branch's
/// recorded bad debt by the same amount.
pub trait VaultBadDebtInterface<AssetId, Balance, Credit> {
	/// Record `amount` of unbacked debt against `collateral_id`.
	fn record_bad_debt(collateral_id: AssetId, amount: Balance) -> DispatchResult;

	/// Burn up to the recorded bad debt of `collateral_id` from `credit` and
	/// return the unconsumed surplus (zero when the credit was fully used).
	fn heal(collateral_id: AssetId, credit: Credit) -> Result<Credit, DispatchError>;
}
