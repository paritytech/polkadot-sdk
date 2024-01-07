pub mod sufficients {
	use sp_runtime::DispatchResult;

	/// Trait for providing the sufficient state of an asset.
	pub trait Inspect<AssetId> {
		/// Returns whether an asset is sufficient or not
		fn is_sufficient(asset_id: AssetId) -> bool;
	}

	/// Trait for mutating the sufficient state of an asset
	pub trait Mutate<AssetId> {
		/// Sets the `is_sufficient` value of an asset.
		///
		/// ### Errors
		///
		/// - [`Unknown`][crate::Error::Unknown] when the asset ID is unknown.
		fn set_sufficient(asset_id: AssetId, is_sufficient: bool) -> DispatchResult;
	}
}
