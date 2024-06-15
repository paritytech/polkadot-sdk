use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use xcm::latest::prelude::*;

pub mod adapter;
pub mod derivatives;
pub mod ops;

pub use adapter::*;
pub use derivatives::*;
pub use ops::*;

pub type NonFungibleAsset = (AssetId, AssetInstance);
