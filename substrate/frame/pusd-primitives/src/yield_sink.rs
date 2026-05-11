//! Branch-aware yield sink trait.
//!
//! The vault pallet mints pUSD interest as a `fungible::Credit`, splits it per
//! `SpYieldShare`, and hands the SP-bound share to a sink that resolves the
//! credit into the branch pool account in one call (`troves.md` §2 / §7.3).

use frame::deps::frame_support::pallet_prelude::DispatchResult;

/// Sink for the SP share of branch-tagged yield. `Credit` is intended to be a
/// `fungible::Credit<AccountId, StableAsset>`; making it a generic parameter
/// avoids depending on the consumer's stable-asset configuration here.
pub trait OnBranchYield<AssetId, Credit> {
	/// Consume `credit` against branch `collateral_id`, decreasing the credit
	/// to zero by depositing into the branch pool account or otherwise
	/// settling it. Implementations must drop or net the credit before
	/// returning to satisfy `OnDropCredit` accounting.
	fn on_branch_yield(collateral_id: AssetId, credit: Credit) -> DispatchResult;
}

/// Convenience no-op implementation: drops the credit on the floor. Useful in
/// runtimes that route 100% of yield via `FeeHandler` instead.
impl<AssetId, Credit> OnBranchYield<AssetId, Credit> for () {
	fn on_branch_yield(_collateral_id: AssetId, _credit: Credit) -> DispatchResult {
		Ok(())
	}
}
