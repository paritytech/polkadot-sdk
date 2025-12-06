// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Cumulus parachain inherent
//!
//! The [`ParachainInherentData`] is the data that is passed by the collator to the parachain
//! runtime. The runtime will use this data to execute messages from other parachains/the relay
//! chain or to read data from the relay chain state. When the parachain is validated by a parachain
//! validator on the relay chain, this data is checked for correctness. If the data passed by the
//! collator to the runtime isn't correct, the parachain candidate is considered invalid.
//!
//! To create a [`ParachainInherentData`] for a specific relay chain block, there exists the
//! `ParachainInherentDataExt` trait in `cumulus-client-parachain-inherent` that helps with this.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use cumulus_primitives_core::{
	relay_chain::{
		vstaging::ApprovedPeerId, BlakeTwo256, Hash as RelayHash, HashT as _, Header as RelayHeader,
	},
	InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use scale_info::TypeInfo;
use sp_inherents::InherentIdentifier;

/// The identifier for the parachain inherent.
pub const PARACHAIN_INHERENT_IDENTIFIER_V0: InherentIdentifier = *b"sysi1337";
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"sysi1338";

/// Legacy ParachainInherentData that is kept around for backward compatibility.
/// Can be removed once we can safely assume that parachain nodes provide the
/// `relay_parent_descendants` and `collator_peer_id` fields.
pub mod v0 {
	use alloc::{collections::BTreeMap, vec::Vec};
	use cumulus_primitives_core::{
		InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
	};
	use scale_info::TypeInfo;

	/// The inherent data that is passed by the collator to the parachain runtime.
	#[derive(
		codec::Encode,
		codec::Decode,
		codec::DecodeWithMemTracking,
		sp_core::RuntimeDebug,
		Clone,
		PartialEq,
		TypeInfo,
	)]
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
		/// were sent. In combination with the rule of no more than one message in a channel per
		/// block, this means `sent_at` is **strictly** greater than the previous one (if any).
		pub horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
	}
}

/// The inherent data that is passed by the collator to the parachain runtime.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
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
	/// Contains the relay parent header and its descendants.
	/// This information is used to ensure that a parachain node builds blocks
	/// at a specified offset from the chain tip rather than directly at the tip.
	pub relay_parent_descendants: Vec<RelayHeader>,
	/// Contains the collator peer ID, which is later sent by the parachain to the
	/// relay chain via a UMP signal to promote the reputation of the given peer ID.
	pub collator_peer_id: Option<ApprovedPeerId>,
}

// Upgrades the ParachainInherentData v0 to the newest format.
impl Into<ParachainInherentData> for v0::ParachainInherentData {
	fn into(self) -> ParachainInherentData {
		ParachainInherentData {
			validation_data: self.validation_data,
			relay_chain_state: self.relay_chain_state,
			downward_messages: self.downward_messages,
			horizontal_messages: self.horizontal_messages,
			relay_parent_descendants: Vec::new(),
			collator_peer_id: None,
		}
	}
}

#[cfg(feature = "std")]
impl ParachainInherentData {
	/// Transforms [`ParachainInherentData`] into [`v0::ParachainInherentData`]. Can be used to
	/// create inherent data compatible with old runtimes.
	fn as_v0(&self) -> v0::ParachainInherentData {
		v0::ParachainInherentData {
			validation_data: self.validation_data.clone(),
			relay_chain_state: self.relay_chain_state.clone(),
			downward_messages: self.downward_messages.clone(),
			horizontal_messages: self.horizontal_messages.clone(),
		}
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for ParachainInherentData {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(PARACHAIN_INHERENT_IDENTIFIER_V0, &self.as_v0())?;
		inherent_data.put_data(INHERENT_IDENTIFIER, &self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
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
	/// Create a new instance initialized to `hash`.
	pub fn new(hash: RelayHash) -> Self {
		Self(hash)
	}

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
