pub mod sufficients {
	use sp_runtime::DispatchResult;

	/// Trait for providing the sufficient state of an asset.
	pub trait Inspect<AssetId> {
		/// Returns whether an asset is sufficient or not.
		fn is_sufficient(asset_id: AssetId) -> bool;
	}

	/// Trait for mutating the sufficient state of an asset
	pub trait Mutate<AssetId> {
		/// Makes the asset to be sufficient.
		///
		/// ### Errors
		///
		/// - [`Unknown`][crate::Error::Unknown] when the asset ID is unknown.
		fn make_sufficient(asset_id: AssetId) -> DispatchResult;

		/// Makes the asset to be insufficient.
		///
		/// ### Errors
		///
		/// - [`Unknown`][crate::Error::Unknown] when the asset ID is unknown.
		fn make_insufficient(asset_id: AssetId) -> DispatchResult;
	}
}
