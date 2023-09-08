// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Definitions, related to bridge with Polkadot Bulletin Chain.

use frame_support::parameter_types;

parameter_types! {
	/// Maximal number of unrewarded relayer entries.
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	/// Maximal number of unconfirmed messages.
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	/// Chain identifier of Polkadot Bulletin Chain.
	pub const PolkadotBulletinChainId: bp_runtime::ChainId = bp_polkadot_bulletin::CHAIN_ID;
}
