use xcm::latest::prelude::*;

pub mod adapter;
pub mod derivatives;
pub mod ops;

pub use adapter::*;
pub use derivatives::*;
pub use ops::*;

pub struct NonFungibleAsset {
	pub id: AssetId,
	pub instance: AssetInstance,
}
