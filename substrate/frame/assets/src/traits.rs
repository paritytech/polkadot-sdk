pub mod sufficiency {
	use sp_runtime::DispatchResult;

	/// Trait for providing the sufficiency of an asset.
	pub trait IsSufficient<AssetId> {
		/// Returns whether an asset is sufficient or not.
		fn is_sufficient(asset_id: AssetId) -> bool;
	}

	/// Trait for mutating the sufficiency of an asset
	pub trait SetSufficiency<AssetId> {
		/// Makes the asset sufficient.
		fn make_sufficient(asset_id: AssetId) -> DispatchResult;

		/// Makes the asset insufficient.
		fn make_insufficient(asset_id: AssetId) -> DispatchResult;
	}
}
