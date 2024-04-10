// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::cli::CliChain;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use relay_substrate_client::{Chain, ChainWithTransactions, Parachain, RelayChain};
use strum::{EnumString, EnumVariantNames};
use substrate_relay_helper::{
	finality::SubstrateFinalitySyncPipeline, messages_lane::SubstrateMessageLane,
	parachains::SubstrateParachainsPipeline,
};

#[derive(Debug, PartialEq, Eq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Supported full bridges (headers + messages).
pub enum FullBridge {
	MillauToRialto,
	RialtoToMillau,
	MillauToRialtoParachain,
	RialtoParachainToMillau,
	BridgeHubRococoToBridgeHubWococo,
	BridgeHubWococoToBridgeHubRococo,
}

impl FullBridge {
	/// Return instance index of the bridge pallet in source runtime.
	pub fn bridge_instance_index(&self) -> u8 {
		match self {
			Self::MillauToRialto => MILLAU_TO_RIALTO_INDEX,
			Self::RialtoToMillau => RIALTO_TO_MILLAU_INDEX,
			Self::MillauToRialtoParachain => MILLAU_TO_RIALTO_PARACHAIN_INDEX,
			Self::RialtoParachainToMillau => RIALTO_PARACHAIN_TO_MILLAU_INDEX,
			Self::BridgeHubRococoToBridgeHubWococo | Self::BridgeHubWococoToBridgeHubRococo =>
				unimplemented!("Relay doesn't support send-message subcommand on bridge hubs"),
		}
	}
}

pub const RIALTO_TO_MILLAU_INDEX: u8 = 0;
pub const MILLAU_TO_RIALTO_INDEX: u8 = 0;
pub const MILLAU_TO_RIALTO_PARACHAIN_INDEX: u8 = 1;
pub const RIALTO_PARACHAIN_TO_MILLAU_INDEX: u8 = 0;

/// Minimal bridge representation that can be used from the CLI.
/// It connects a source chain to a target chain.
pub trait CliBridgeBase: Sized {
	/// The source chain.
	type Source: Chain + CliChain;
	/// The target chain.
	type Target: ChainWithTransactions + CliChain;
}

/// Bridge representation that can be used from the CLI for relaying headers
/// from a relay chain to a relay chain.
pub trait RelayToRelayHeadersCliBridge: CliBridgeBase {
	/// Finality proofs synchronization pipeline.
	type Finality: SubstrateFinalitySyncPipeline<
		SourceChain = Self::Source,
		TargetChain = Self::Target,
	>;
}

/// Bridge representation that can be used from the CLI for relaying headers
/// from a parachain to a relay chain.
pub trait ParachainToRelayHeadersCliBridge: CliBridgeBase
where
	Self::Source: Parachain,
{
	// The `CliBridgeBase` type represents the parachain in this situation.
	// We need to add an extra type for the relay chain.
	type SourceRelay: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
		+ CliChain
		+ RelayChain;
	/// Finality proofs synchronization pipeline (source parachain -> target).
	type ParachainFinality: SubstrateParachainsPipeline<
		SourceRelayChain = Self::SourceRelay,
		SourceParachain = Self::Source,
		TargetChain = Self::Target,
	>;
	/// Finality proofs synchronization pipeline (source relay chain -> target).
	type RelayFinality: SubstrateFinalitySyncPipeline<
		SourceChain = Self::SourceRelay,
		TargetChain = Self::Target,
	>;
}

/// Bridge representation that can be used from the CLI for relaying messages.
pub trait MessagesCliBridge: CliBridgeBase {
	/// The Source -> Destination messages synchronization pipeline.
	type MessagesLane: SubstrateMessageLane<SourceChain = Self::Source, TargetChain = Self::Target>;
}
