//! XCM utilities to work with NFT-like entities (unique instances).
//! The adapters and other utility types use the [`asset_ops`](frame_support::traits::tokens::asset_ops) traits.

use xcm::latest::prelude::*;

pub mod adapter;
pub mod derivatives;
pub mod ops;

pub use adapter::*;
pub use derivatives::*;
pub use ops::*;

pub type NonFungibleAsset = (AssetId, AssetInstance);
