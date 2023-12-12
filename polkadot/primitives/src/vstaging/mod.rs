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

//! Staging Primitives.

// Put any primitives used by staging APIs functions here

use bitvec::vec::BitVec;

/// Bit indices in the `HostConfiguration.node_features` that correspond to different node features.
/// The maximum bit that can be currently set/unset via the `set_node_feature` extrinsic is 255.
pub type NodeFeatures = BitVec<u8, bitvec::order::Lsb0>;

/// Module containing feature-specific bit indices into the `NodeFeatures` bitvec.
pub mod node_features {
	/// A feature index used to indentify a bit into the node_features array stored
	/// in the HostConfiguration.
	#[repr(u8)]
	pub enum FeatureIndex {
		/// Tells if tranch0 assignments could be sent in a single certificate.
		/// Reserved for: `<https://github.com/paritytech/polkadot-sdk/issues/628>`
		EnableAssignmentsV2 = 0,
		/// Index of the availability chunk shuffling feature bit.
		AvailabilityChunkShuffling = 1,
		/// First unassigned feature bit.
		/// Every time a new feature flag is assigned it should take this value.
		/// and this should be incremented.
		FirstUnassigned = 2,
	}
}
