// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Instructions for XCM v6.

use bounded_collections::BoundedVec;
use codec::{Decode, Encode};
use educe::Educe;
use scale_info::TypeInfo;

use crate::v5::{
	Asset, AssetFilter, AssetTransferFilter, Assets, Error, Hint, HintNumVariants,
	InteriorLocation, Junction, Location, MaybeErrorCode, NetworkId, OriginKind, QueryId,
	QueryResponseInfo, Response, Weight, WeightLimit, Xcm,
};
use crate::DoubleEncoded;

mod assets;
pub use assets::*;

mod notifications;
pub use notifications::*;

mod origin;
pub use origin::*;

mod controls;
pub use controls::*;

mod report;
pub use report::*;

mod expect;
pub use expect::*;

mod fees;
pub use fees::*;

mod versions;
pub use versions::*;

mod query;
pub use query::*;

mod misc;
pub use misc::*;
