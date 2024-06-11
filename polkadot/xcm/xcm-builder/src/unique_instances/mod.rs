use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use xcm::latest::prelude::*;

pub mod adapter;
pub mod derivatives;
pub mod ops;

pub use adapter::*;
pub use derivatives::*;
pub use ops::*;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct NonFungibleAsset {
	pub id: AssetId,
	pub instance: AssetInstance,
}
