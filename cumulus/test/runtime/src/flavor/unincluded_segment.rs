// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
//
// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Unincluded segment capacity for [`cumulus_pallet_aura_ext::FixedVelocityConsensusHook`].

use super::{block_velocity::BLOCK_PROCESSING_VELOCITY, relay_parent::RELAY_PARENT_OFFSET};

#[cfg(feature = "async-backing")]
pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;

#[cfg(all(feature = "sync-backing", not(feature = "async-backing")))]
pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 1;

/// We need `VELOCITY * 3`, because the block flow is the following:
///
/// - Collator produces the block(s) on relay chain block `X`
/// - In the mean time the relay chain is building block `X + 1`
/// - The collator sends the collation to the relay chain and it gets backed on chain in relay block
///   `X + 2`
/// - The collation then gets included on chain in relay block `X + 3`
/// - As we are building on `RELAY_PARENT_OFFSET` old relay parents, the included block from the
///   parachain is also `RELAY_PARENT_OFFSET` relay blocks older (one relay block may contains
///   multiple parachain blocks).
#[cfg(all(not(feature = "sync-backing"), not(feature = "async-backing")))]
pub const UNINCLUDED_SEGMENT_CAPACITY: u32 =
	BLOCK_PROCESSING_VELOCITY * (3 + RELAY_PARENT_OFFSET);
