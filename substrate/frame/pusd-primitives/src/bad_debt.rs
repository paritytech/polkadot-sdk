//! Bad-debt healing trait (`troves.md` §10.5).

use frame::deps::frame_support::pallet_prelude::DispatchResult;

/// One-shot bad-debt healing call. The orchestrator (typically
/// `pallet-stability-pool`) withdraws from the Insurance Fund as a
/// `fungible::Credit` and hands it here; the implementation rescinds the
/// underlying pUSD and decrements `BranchStates[c].bad_debt`.
pub trait VaultBadDebtInterface<AssetId, Credit> {
	fn heal(collateral_id: AssetId, credit: Credit) -> DispatchResult;
}
