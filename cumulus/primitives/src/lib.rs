// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Cumulus related primitive types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

pub use polkadot_core_primitives::InboundDownwardMessage;
pub use polkadot_parachain::primitives::{Id as ParaId, UpwardMessage, ValidationParams};
pub use polkadot_primitives::v1::{
	PersistedValidationData, AbridgedHostConfiguration, AbridgedHrmpChannel,
};

#[cfg(feature = "std")]
pub mod genesis;

/// A module that re-exports relevant relay chain definitions.
pub mod relay_chain {
	pub use polkadot_core_primitives::*;
	pub use polkadot_primitives::v1;
	pub use polkadot_primitives::v1::well_known_keys;
}

/// An inbound HRMP message.
pub type InboundHrmpMessage = polkadot_primitives::v1::InboundHrmpMessage<relay_chain::BlockNumber>;

/// And outbound HRMP message
pub type OutboundHrmpMessage = polkadot_primitives::v1::OutboundHrmpMessage<ParaId>;

/// Identifiers and types related to Cumulus Inherents
pub mod inherents {
	use super::{InboundDownwardMessage, InboundHrmpMessage, ParaId};
	use sp_inherents::InherentIdentifier;
	use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

	/// The identifier for the parachain-system inherent.
	pub const SYSTEM_INHERENT_IDENTIFIER: InherentIdentifier = *b"sysi1337";

	/// The payload that system inherent carries.
	#[derive(codec::Encode, codec::Decode, sp_core::RuntimeDebug, Clone, PartialEq)]
	pub struct SystemInherentData {
		pub validation_data: crate::PersistedValidationData,
		/// A storage proof of a predefined set of keys from the relay-chain.
		///
		/// Specifically this witness contains the data for:
		///
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
}

/// Well known keys for values in the storage.
pub mod well_known_keys {
	/// The storage key for the upward messages.
	///
	/// The upward messages are stored as SCALE encoded `Vec<UpwardMessage>`.
	pub const UPWARD_MESSAGES: &'static [u8] = b":cumulus_upward_messages:";

	/// Current validation data.
	pub const VALIDATION_DATA: &'static [u8] = b":cumulus_validation_data:";

	/// Code upgarde (set as appropriate by a pallet).
	pub const NEW_VALIDATION_CODE: &'static [u8] = b":cumulus_new_validation_code:";

	/// The storage key with which the runtime passes outbound HRMP messages it wants to send to the
	/// PVF.
	///
	/// The value is stored as SCALE encoded `Vec<OutboundHrmpMessage>`
	pub const HRMP_OUTBOUND_MESSAGES: &'static [u8] = b":cumulus_hrmp_outbound_messages:";

	/// The storage key for communicating the HRMP watermark from the runtime to the PVF. Cleared by
	/// the runtime each block and set after message inclusion, but only if there were messages.
	///
	/// The value is stored as SCALE encoded relay-chain's `BlockNumber`.
	pub const HRMP_WATERMARK: &'static [u8] = b":cumulus_hrmp_watermark:";

	/// The storage key for the processed downward messages.
	///
	/// The value is stored as SCALE encoded `u32`.
	pub const PROCESSED_DOWNWARD_MESSAGES: &'static [u8] = b":cumulus_processed_downward_messages:";
}

/// Something that should be called when a downward message is received.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait DownwardMessageHandler {
	/// Handle the given downward message.
	fn handle_downward_message(msg: InboundDownwardMessage);
}

/// Something that should be called when an HRMP message is received.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait HrmpMessageHandler {
	/// Handle the given HRMP message.
	fn handle_hrmp_message(sender: ParaId, msg: InboundHrmpMessage);
}

/// Something that should be called when sending an upward message.
pub trait UpwardMessageSender {
	/// Send the given upward message.
	fn send_upward_message(msg: UpwardMessage) -> Result<(), ()>;
}

/// Something that should be called when sending an HRMP message.
pub trait HrmpMessageSender {
	/// Send the given HRMP message.
	fn send_hrmp_message(msg: OutboundHrmpMessage) -> Result<(), ()>;
}

/// A trait which is called when the validation data is set.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnValidationData {
	fn on_validation_data(data: PersistedValidationData);
}
