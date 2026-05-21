//! Oracle trait surface.
//!
//! The pUSD vault pallet treats price/freshness validation as the oracle
//! pallet's responsibility. The trait shape mirrors
//! that: a single `provide_price` accessor that returns a normalised price and
//! the timestamp it was observed at, or fails with a `DispatchError` when the
//! price is stale or unavailable.

use frame::deps::{frame_support::pallet_prelude::DispatchError, sp_runtime::FixedU128};

/// One observed price and the time at which the oracle observed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceFeed<Moment> {
	pub price: FixedU128,
	pub observed_at: Moment,
}

/// Read-only access to a normalised price for a given collateral.
pub trait ProvidePrice {
	type AssetId;
	type Moment;

	/// Latest price for `collateral_id`. Implementations should return
	/// `Err(_)` if the price is stale or unavailable so the calling pallet can
	/// transition the branch into `Frozen` mode.
	fn provide_price(
		collateral_id: &Self::AssetId,
	) -> Result<PriceFeed<Self::Moment>, DispatchError>;
}
