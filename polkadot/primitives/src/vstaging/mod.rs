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
pub use crate::v6::*;
use sp_std::prelude::*;

use parity_scale_codec::{Decode, Encode};
use primitives::RuntimeDebug;
use scale_info::TypeInfo;

/// Approval voting configuration parameters
#[derive(
	RuntimeDebug,
	Copy,
	Clone,
	PartialEq,
	Encode,
	Decode,
	TypeInfo,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct ApprovalVotingParams {
	/// The maximum number of candidates `approval-voting` can vote for with
	/// a single signatures.
	///
	/// Setting it to 1, means we send the approval as soon as we have it available.
	pub max_approval_coalesce_count: u32,
}

impl Default for ApprovalVotingParams {
	fn default() -> Self {
		Self { max_approval_coalesce_count: 1 }
	}
}

use bitvec::vec::BitVec;

/// Bit indices in the `HostConfiguration.node_features` that correspond to different node features.
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
		/// First unassigned feature bit.
		/// Every time a new feature flag is assigned it should take this value.
		/// and this should be incremented.
		FirstUnassigned = 1,
	}
}
