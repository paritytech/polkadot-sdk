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

//! Price oracle bounds and handles for cumulus parachain collators.

use super::aura::AuraIdT;
use sp_price_oracle::runtime_api::{PriceOracleApi, PriceOracleMarketApi};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

/// Convenience trait for defining the bounds of a parachain runtime whose collators run the
/// price oracle. The signer key is the Aura key.
pub trait PriceOracleRuntimeApi<Block: BlockT, AuraId: AuraIdT>:
	PriceOracleApi<Block, AuraId::BoundedPublic> + PriceOracleMarketApi<Block>
{
}

impl<T, Block: BlockT, AuraId: AuraIdT> PriceOracleRuntimeApi<Block, AuraId> for T where
	T: PriceOracleApi<Block, AuraId::BoundedPublic> + PriceOracleMarketApi<Block>
{
}

/// Network handles of the price oracle gossip protocol.
///
/// Created by `start_node` when the protocol is registered with the network, and consumed by
/// `StartConsensus::start_consensus` to spawn the price oracle service.
pub struct PriceOracleNetwork<Block: BlockT> {
	/// Notification service of the protocol.
	pub notification_service: Box<dyn sc_network::service::traits::NotificationService>,
	/// Name of the protocol.
	pub protocol_name: sc_network::ProtocolName,
	/// The network service.
	pub network: Arc<dyn sc_network::service::traits::NetworkService>,
	/// The sync service.
	pub sync_service: Arc<sc_network_sync::SyncingService<Block>>,
	/// Prometheus registry of the node.
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
}
