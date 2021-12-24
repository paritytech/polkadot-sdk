// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus parachain inherent
//!
//! The [`ParachainInherentData`] is the data that is passed by the collator to the parachain runtime.
//! The runtime will use this data to execute messages from other parachains/the relay chain or to
//! read data from the relay chain state. When the parachain is validated by a parachain validator on
//! the relay chain, this data is checked for correctnes. If the data passed by the collator to the
//! runtime isn't correct, the parachain candidate is considered invalid.
//!
//! Use [`ParachainInherentData::create_at`] to create the [`ParachainInherentData`] at a given
//! relay chain block to include it in a parachain block.

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_primitives_core::{
	relay_chain::{BlakeTwo256, Hash as RelayHash, HashT as _},
	InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};

use scale_info::TypeInfo;
use sp_inherents::InherentIdentifier;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[cfg(feature = "std")]
mod client_side;
#[cfg(feature = "std")]
pub use client_side::*;
#[cfg(feature = "std")]
mod mock;
#[cfg(feature = "std")]
pub use mock::{MockValidationDataInherentDataProvider, MockXcmConfig};

/// The identifier for the parachain inherent.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"sysi1337";

/// The inherent data that is passed by the collator to the parachain runtime.
#[derive(codec::Encode, codec::Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct ParachainInherentData {
	pub validation_data: PersistedValidationData,
	/// A storage proof of a predefined set of keys from the relay-chain.
	///
	/// Specifically this witness contains the data for:
	///
	/// - the current slot number at the given relay parent
	/// - active host configuration as per the relay parent,
	/// - the relay dispatch queue sizes
	/// - the list of egress HRMP channels (in the list of recipients form)
	/// - the metadata for the egress HRMP channels
	pub relay_chain_state: sp_trie::StorageProof,
	/// Downward messages in the order they were sent.
	pub downward_messages: Vec<InboundDownwardMessage>,
	/// HRMP messages grouped by channels. The messages in the inner vec must be in order they
	/// were sent. In combination with the rule of no more than one message in a channel per block,
	/// this means `sent_at` is **strictly** greater than the previous one (if any).
	pub horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
}

/// This struct provides ability to extend a message queue chain (MQC) and compute a new head.
///
/// MQC is an instance of a [hash chain] applied to a message queue. Using a hash chain it's
/// possible to represent a sequence of messages using only a single hash.
///
/// A head for an empty chain is agreed to be a zero hash.
///
/// An instance is used to track either DMP from the relay chain or HRMP across a channel.
/// But a given instance is never used to track both. Therefore, you should call either
/// `extend_downward` or `extend_hrmp`, but not both methods on a single instance.
///
/// [hash chain]: https://en.wikipedia.org/wiki/Hash_chain
#[derive(Default, Clone, codec::Encode, codec::Decode, scale_info::TypeInfo)]
pub struct MessageQueueChain(RelayHash);

impl MessageQueueChain {
	/// Extend the hash chain with an HRMP message. This method should be used only when
	/// this chain is tracking HRMP.
	pub fn extend_hrmp(&mut self, horizontal_message: &InboundHrmpMessage) -> &mut Self {
		let prev_head = self.0;
		self.0 = BlakeTwo256::hash_of(&(
			prev_head,
			horizontal_message.sent_at,
			BlakeTwo256::hash_of(&horizontal_message.data),
		));
		self
	}

	/// Extend the hash chain with a downward message. This method should be used only when
	/// this chain is tracking DMP.
	pub fn extend_downward(&mut self, downward_message: &InboundDownwardMessage) -> &mut Self {
		let prev_head = self.0;
		self.0 = BlakeTwo256::hash_of(&(
			prev_head,
			downward_message.sent_at,
			BlakeTwo256::hash_of(&downward_message.msg),
		));
		self
	}

	/// Return the current mead of the message queue hash chain.
	/// This is agreed to be the zero hash for an empty chain.
	pub fn head(&self) -> RelayHash {
		self.0
	}
}
