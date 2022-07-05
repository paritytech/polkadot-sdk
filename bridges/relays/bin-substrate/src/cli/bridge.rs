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
use substrate_relay_helper::finality::SubstrateFinalitySyncPipeline;

#[derive(Debug, PartialEq, Eq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Supported full bridges (headers + messages).
pub enum FullBridge {
	MillauToRialto,
	RialtoToMillau,
	RococoToWococo,
	WococoToRococo,
	KusamaToPolkadot,
	PolkadotToKusama,
	MillauToRialtoParachain,
	RialtoParachainToMillau,
}

impl FullBridge {
	/// Return instance index of the bridge pallet in source runtime.
	pub fn bridge_instance_index(&self) -> u8 {
		match self {
			Self::MillauToRialto => MILLAU_TO_RIALTO_INDEX,
			Self::RialtoToMillau => RIALTO_TO_MILLAU_INDEX,
			Self::RococoToWococo => ROCOCO_TO_WOCOCO_INDEX,
			Self::WococoToRococo => WOCOCO_TO_ROCOCO_INDEX,
			Self::KusamaToPolkadot => KUSAMA_TO_POLKADOT_INDEX,
			Self::PolkadotToKusama => POLKADOT_TO_KUSAMA_INDEX,
			Self::MillauToRialtoParachain => MILLAU_TO_RIALTO_PARACHAIN_INDEX,
			Self::RialtoParachainToMillau => RIALTO_PARACHAIN_TO_MILLAU_INDEX,
		}
	}
}

pub const RIALTO_TO_MILLAU_INDEX: u8 = 0;
pub const MILLAU_TO_RIALTO_INDEX: u8 = 0;
pub const ROCOCO_TO_WOCOCO_INDEX: u8 = 0;
pub const WOCOCO_TO_ROCOCO_INDEX: u8 = 0;
pub const KUSAMA_TO_POLKADOT_INDEX: u8 = 0;
pub const POLKADOT_TO_KUSAMA_INDEX: u8 = 0;
pub const MILLAU_TO_RIALTO_PARACHAIN_INDEX: u8 = 1;
pub const RIALTO_PARACHAIN_TO_MILLAU_INDEX: u8 = 0;

/// The macro allows executing bridge-specific code without going fully generic.
///
/// It matches on the [`FullBridge`] enum, sets bridge-specific types or imports and injects
/// the `$generic` code at every variant.
#[macro_export]
macro_rules! select_full_bridge {
	($bridge: expr, $generic: tt) => {
		match $bridge {
			FullBridge::MillauToRialto => {
				type Source = relay_millau_client::Millau;
				#[allow(dead_code)]
				type Target = relay_rialto_client::Rialto;

				// Derive-account
				#[allow(unused_imports)]
				use bp_rialto::derive_account_from_millau_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::millau_messages_to_rialto::MillauMessagesToRialto as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::RialtoToMillau => {
				type Source = relay_rialto_client::Rialto;
				#[allow(dead_code)]
				type Target = relay_millau_client::Millau;

				// Derive-account
				#[allow(unused_imports)]
				use bp_millau::derive_account_from_rialto_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::rialto_messages_to_millau::RialtoMessagesToMillau as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::RococoToWococo => {
				type Source = relay_rococo_client::Rococo;
				#[allow(dead_code)]
				type Target = relay_wococo_client::Wococo;

				// Derive-account
				#[allow(unused_imports)]
				use bp_wococo::derive_account_from_rococo_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::rococo_messages_to_wococo::RococoMessagesToWococo as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_wococo::TO_WOCOCO_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::WococoToRococo => {
				type Source = relay_wococo_client::Wococo;
				#[allow(dead_code)]
				type Target = relay_rococo_client::Rococo;

				// Derive-account
				#[allow(unused_imports)]
				use bp_rococo::derive_account_from_wococo_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::wococo_messages_to_rococo::WococoMessagesToRococo as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_rococo::TO_ROCOCO_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::KusamaToPolkadot => {
				type Source = relay_kusama_client::Kusama;
				#[allow(dead_code)]
				type Target = relay_polkadot_client::Polkadot;

				// Derive-account
				#[allow(unused_imports)]
				use bp_polkadot::derive_account_from_kusama_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::kusama_messages_to_polkadot::KusamaMessagesToPolkadot as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_polkadot::TO_POLKADOT_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::PolkadotToKusama => {
				type Source = relay_polkadot_client::Polkadot;
				#[allow(dead_code)]
				type Target = relay_kusama_client::Kusama;

				// Derive-account
				#[allow(unused_imports)]
				use bp_kusama::derive_account_from_polkadot_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::polkadot_messages_to_kusama::PolkadotMessagesToKusama as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_kusama::TO_KUSAMA_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			},
			FullBridge::MillauToRialtoParachain => {
				type Source = relay_millau_client::Millau;
				#[allow(dead_code)]
				type Target = relay_rialto_parachain_client::RialtoParachain;

				// Derive-account
				#[allow(unused_imports)]
				use bp_rialto_parachain::derive_account_from_millau_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::millau_messages_to_rialto_parachain::MillauMessagesToRialtoParachain as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_rialto_parachain::TO_RIALTO_PARACHAIN_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			}
			FullBridge::RialtoParachainToMillau => {
				type Source = relay_rialto_parachain_client::RialtoParachain;
				#[allow(dead_code)]
				type Target = relay_millau_client::Millau;

				// Derive-account
				#[allow(unused_imports)]
				use bp_millau::derive_account_from_rialto_parachain_id as derive_account;

				// Relay-messages
				#[allow(unused_imports)]
				use $crate::chains::rialto_parachain_messages_to_millau::RialtoParachainMessagesToMillau as MessagesLane;

				// Send-message / Estimate-fee
				#[allow(unused_imports)]
				use bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD as ESTIMATE_MESSAGE_FEE_METHOD;

				$generic
			}
		}
	};
}

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

/// Bridge representation that can be used from the CLI.
pub trait CliBridge: CliBridgeBase {
	/// Finality proofs synchronization pipeline.
	type Finality: SubstrateFinalitySyncPipeline<
		SourceChain = Self::Source,
		TargetChain = Self::Target,
		TransactionSignScheme = Self::Target,
	>;
}

//// `Millau` to `Rialto` bridge definition.
pub struct MillauToRialtoCliBridge {}

impl CliBridgeBase for MillauToRialtoCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_client::Rialto;
}

impl CliBridge for MillauToRialtoCliBridge {
	type Finality = crate::chains::millau_headers_to_rialto::MillauFinalityToRialto;
}

//// `Rialto` to `Millau` bridge definition.
pub struct RialtoToMillauCliBridge {}

impl CliBridgeBase for RialtoToMillauCliBridge {
	type Source = relay_rialto_client::Rialto;
	type Target = relay_millau_client::Millau;
}

impl CliBridge for RialtoToMillauCliBridge {
	type Finality = crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau;
}

//// `Westend` to `Millau` bridge definition.
pub struct WestendToMillauCliBridge {}

impl CliBridgeBase for WestendToMillauCliBridge {
	type Source = relay_westend_client::Westend;
	type Target = relay_millau_client::Millau;
}

impl CliBridge for WestendToMillauCliBridge {
	type Finality = crate::chains::westend_headers_to_millau::WestendFinalityToMillau;
}

//// `Rococo` to `Wococo` bridge definition.
pub struct RococoToWococoCliBridge {}

impl CliBridgeBase for RococoToWococoCliBridge {
	type Source = relay_rococo_client::Rococo;
	type Target = relay_wococo_client::Wococo;
}

impl CliBridge for RococoToWococoCliBridge {
	type Finality = crate::chains::rococo_headers_to_wococo::RococoFinalityToWococo;
}

//// `Wococo` to `Rococo` bridge definition.
pub struct WococoToRococoCliBridge {}

impl CliBridgeBase for WococoToRococoCliBridge {
	type Source = relay_wococo_client::Wococo;
	type Target = relay_rococo_client::Rococo;
}

impl CliBridge for WococoToRococoCliBridge {
	type Finality = crate::chains::wococo_headers_to_rococo::WococoFinalityToRococo;
}

//// `Kusama` to `Polkadot` bridge definition.
pub struct KusamaToPolkadotCliBridge {}

impl CliBridgeBase for KusamaToPolkadotCliBridge {
	type Source = relay_kusama_client::Kusama;
	type Target = relay_polkadot_client::Polkadot;
}

impl CliBridge for KusamaToPolkadotCliBridge {
	type Finality = crate::chains::kusama_headers_to_polkadot::KusamaFinalityToPolkadot;
}

//// `Polkadot` to `Kusama`  bridge definition.
pub struct PolkadotToKusamaCliBridge {}

impl CliBridgeBase for PolkadotToKusamaCliBridge {
	type Source = relay_polkadot_client::Polkadot;
	type Target = relay_kusama_client::Kusama;
}

impl CliBridge for PolkadotToKusamaCliBridge {
	type Finality = crate::chains::polkadot_headers_to_kusama::PolkadotFinalityToKusama;
}

//// `Millau` to `RialtoParachain`  bridge definition.
pub struct MillauToRialtoParachainCliBridge {}

impl CliBridgeBase for MillauToRialtoParachainCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_parachain_client::RialtoParachain;
}

impl CliBridge for MillauToRialtoParachainCliBridge {
	type Finality =
		crate::chains::millau_headers_to_rialto_parachain::MillauFinalityToRialtoParachain;
}

//// `RialtoParachain` to `Millau` bridge definition.
pub struct RialtoParachainToMillauCliBridge {}

impl CliBridgeBase for RialtoParachainToMillauCliBridge {
	type Source = relay_rialto_parachain_client::RialtoParachain;
	type Target = relay_millau_client::Millau;
}

//// `WestendParachain` to `Millau` bridge definition.
pub struct WestmintToMillauCliBridge {}

impl CliBridgeBase for WestmintToMillauCliBridge {
	type Source = relay_westend_client::Westmint;
	type Target = relay_millau_client::Millau;
}
