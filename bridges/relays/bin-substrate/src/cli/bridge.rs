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
use relay_substrate_client::{AccountKeyPairOf, Chain, TransactionSignScheme};
use strum::{EnumString, EnumVariantNames};
use substrate_relay_helper::{
	finality::SubstrateFinalitySyncPipeline, messages_lane::SubstrateMessageLane,
};

#[derive(Debug, PartialEq, Eq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Supported full bridges (headers + messages).
pub enum FullBridge {
	MillauToRialto,
	RialtoToMillau,
	MillauToRialtoParachain,
	RialtoParachainToMillau,
}

impl FullBridge {
	/// Return instance index of the bridge pallet in source runtime.
	pub fn bridge_instance_index(&self) -> u8 {
		match self {
			Self::MillauToRialto => MILLAU_TO_RIALTO_INDEX,
			Self::RialtoToMillau => RIALTO_TO_MILLAU_INDEX,
			Self::MillauToRialtoParachain => MILLAU_TO_RIALTO_PARACHAIN_INDEX,
			Self::RialtoParachainToMillau => RIALTO_PARACHAIN_TO_MILLAU_INDEX,
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
	type Target: Chain
		+ TransactionSignScheme<Chain = Self::Target>
		+ CliChain<KeyPair = AccountKeyPairOf<Self::Target>>;
}

/// Bridge representation that can be used from the CLI for relaying headers.
pub trait HeadersCliBridge: CliBridgeBase {
	/// Finality proofs synchronization pipeline.
	type Finality: SubstrateFinalitySyncPipeline<
		SourceChain = Self::Source,
		TargetChain = Self::Target,
		TransactionSignScheme = Self::Target,
	>;
}

/// Bridge representation that can be used from the CLI for relaying messages.
pub trait MessagesCliBridge: CliBridgeBase {
	/// Name of the runtime method used to estimate the message dispatch and delivery fee for the
	/// defined bridge.
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str;
	/// The Source -> Destination messages synchronization pipeline.
	type MessagesLane: SubstrateMessageLane<
		SourceChain = Self::Source,
		TargetChain = Self::Target,
		SourceTransactionSignScheme = Self::Source,
		TargetTransactionSignScheme = Self::Target,
	>;
}

//// `Millau` to `Rialto` bridge definition.
pub struct MillauToRialtoCliBridge {}

impl CliBridgeBase for MillauToRialtoCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_client::Rialto;
}

impl HeadersCliBridge for MillauToRialtoCliBridge {
	type Finality = crate::chains::millau_headers_to_rialto::MillauFinalityToRialto;
}

impl MessagesCliBridge for MillauToRialtoCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane = crate::chains::millau_messages_to_rialto::MillauMessagesToRialto;
}

//// `Rialto` to `Millau` bridge definition.
pub struct RialtoToMillauCliBridge {}

impl CliBridgeBase for RialtoToMillauCliBridge {
	type Source = relay_rialto_client::Rialto;
	type Target = relay_millau_client::Millau;
}

impl HeadersCliBridge for RialtoToMillauCliBridge {
	type Finality = crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau;
}

impl MessagesCliBridge for RialtoToMillauCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane = crate::chains::rialto_messages_to_millau::RialtoMessagesToMillau;
}

//// `Westend` to `Millau` bridge definition.
pub struct WestendToMillauCliBridge {}

impl CliBridgeBase for WestendToMillauCliBridge {
	type Source = relay_westend_client::Westend;
	type Target = relay_millau_client::Millau;
}

impl HeadersCliBridge for WestendToMillauCliBridge {
	type Finality = crate::chains::westend_headers_to_millau::WestendFinalityToMillau;
}

//// `Millau` to `RialtoParachain`  bridge definition.
pub struct MillauToRialtoParachainCliBridge {}

impl CliBridgeBase for MillauToRialtoParachainCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_parachain_client::RialtoParachain;
}

impl HeadersCliBridge for MillauToRialtoParachainCliBridge {
	type Finality =
		crate::chains::millau_headers_to_rialto_parachain::MillauFinalityToRialtoParachain;
}

impl MessagesCliBridge for MillauToRialtoParachainCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_rialto_parachain::TO_RIALTO_PARACHAIN_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane =
		crate::chains::millau_messages_to_rialto_parachain::MillauMessagesToRialtoParachain;
}

//// `RialtoParachain` to `Millau` bridge definition.
pub struct RialtoParachainToMillauCliBridge {}

impl CliBridgeBase for RialtoParachainToMillauCliBridge {
	type Source = relay_rialto_parachain_client::RialtoParachain;
	type Target = relay_millau_client::Millau;
}

impl MessagesCliBridge for RialtoParachainToMillauCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane =
		crate::chains::rialto_parachain_messages_to_millau::RialtoParachainMessagesToMillau;
}

//// `WestendParachain` to `Millau` bridge definition.
pub struct WestmintToMillauCliBridge {}

impl CliBridgeBase for WestmintToMillauCliBridge {
	type Source = relay_westend_client::Westmint;
	type Target = relay_millau_client::Millau;
}
