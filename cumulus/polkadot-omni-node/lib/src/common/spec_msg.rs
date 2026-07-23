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

//! Speculative Messaging node wiring.
//!
//! Every node serves the sender side — the archive of the own chain's sends
//! behind the `/spec-msg/exchange` request-response protocol — and every
//! collator additionally runs the receiver side: the relay `Provides`
//! monitor triggering the fetch pipeline, whose verified [`SpecMsgPool`]
//! feeds the `specmsg0` inherent provider and the candidate's POV lift
//! assembler (both hooked up by the consensus wiring in [`crate::nodes`]).
//!
//! All components version-gate on the `SpecMsgApi` runtime API and stay
//! idle for runtimes that do not participate — wiring them is free for
//! everything else.
//!
//! Peer sourcing is the MVP mechanism: `--spec-msg-source-peer` supplies
//! static addresses of the source chains' collators (the two ends belong
//! to *different* parachains, so the own chain's peer set never contains
//! them). DHT-based discovery of a source's collators plugs in behind the
//! same [`SourcePeers`] seam later.
//!
//! [`SourcePeers`]: cumulus_client_spec_msg::SourcePeers

use crate::common::{types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock};
use cumulus_client_spec_msg::{
	lift_assembler, run_relay_provides_monitor, run_spec_msg_archiver, run_spec_msg_discovery,
	run_spec_msg_fetcher, BootnodeSourceDiscovery, PeerRegistry, SourceDiscovery, SpecMsgArchive,
	SpecMsgPool, SpecMsgRequestHandler, DISCOVERY_REFRESH_INTERVAL,
};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::LiftsBySource;
use cumulus_relay_chain_interface::RelayChainInterface;
use parking_lot::RwLock;
use sc_network::{
	config::FullNetworkConfiguration, service::traits::NetworkService, NetworkBackend,
};
use sc_service::TaskManager;
use std::{collections::HashMap, sync::Arc};

/// Capacity of the monitor → fetcher trigger channel. Triggers are tiny
/// `(source, root)` tuples; the bound only guards against a wedged fetcher.
const EVENTS_CHANNEL_CAPACITY: usize = 1_024;

/// The receiver-side state the consensus wiring hooks into block authoring:
/// the verified pool (for the `specmsg0` inherent provider) and the lift
/// assembler for `CollatorService`.
pub(crate) struct SpecMsgDeps<Block> {
	/// The fetch pipeline's verified pool, shared with the inherent
	/// provider.
	pub pool: Arc<SpecMsgPool>,
	/// Assembles the candidate's POV lifts and its synthesized `Requires`
	/// commitment signal from the built blocks' consumption records; pass
	/// to `CollatorService::with_spec_msg_lift_assembler`.
	pub lift_assembler: Arc<dyn Fn(&[Block]) -> (LiftsBySource, Option<Vec<u8>>) + Send + Sync>,
}

impl<Block> Clone for SpecMsgDeps<Block> {
	fn clone(&self) -> Self {
		Self { pool: self.pool.clone(), lift_assembler: self.lift_assembler.clone() }
	}
}

/// The parts that must exist before the network is built: the sender-side
/// archive and the `/spec-msg/exchange` protocol registration. Created by
/// [`new_spec_msg_protocol`], consumed by [`Self::start`] once the network
/// and the relay chain interface are up.
pub(crate) struct SpecMsgProtocol<Block: NodeBlock, RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	archive: Arc<RwLock<SpecMsgArchive<Block, ParachainClient<Block, RuntimeApi>>>>,
	handler: SpecMsgRequestHandler<Block, ParachainClient<Block, RuntimeApi>>,
}

/// Loads the sender-side archive (aux-store-mirrored, restart-safe) and
/// registers the `/spec-msg/exchange` request-response protocol on
/// `net_config`.
pub(crate) fn new_spec_msg_protocol<Block, RuntimeApi, Net>(
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	net_config: &mut FullNetworkConfiguration<Block, Block::Hash, Net>,
) -> sc_service::error::Result<SpecMsgProtocol<Block, RuntimeApi>>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Net: NetworkBackend<Block, Block::Hash>,
{
	let archive = SpecMsgArchive::load(client)
		.map_err(|error| sc_service::Error::Application(Box::new(error)))?;
	let archive = Arc::new(RwLock::new(archive));
	let (handler, protocol_config) = SpecMsgRequestHandler::new::<Net>(archive.clone());
	net_config.add_request_response_protocol(protocol_config);
	Ok(SpecMsgProtocol { archive, handler })
}

impl<Block, RuntimeApi> SpecMsgProtocol<Block, RuntimeApi>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	/// Spawns the spec-msg tasks: the exchange request handler and the
	/// archiver on every node; the relay `Provides` monitor and the fetch
	/// pipeline on collators, whose [`SpecMsgDeps`] are returned for the
	/// consensus wiring.
	///
	/// `source_peers` are the raw `--spec-msg-source-peer` values
	/// (`<para-id>=<multiaddr-with-/p2p/-peer-id>`); malformed entries fail
	/// node startup loudly.
	pub(crate) fn start(
		self,
		task_manager: &TaskManager,
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		network: Arc<dyn NetworkService>,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		relay_chain_network: Arc<dyn NetworkService>,
		para_id: ParaId,
		validator: bool,
		source_peers: &[String],
		source_genesis: &[String],
	) -> sc_service::error::Result<Option<SpecMsgDeps<Block>>> {
		let spawner = task_manager.spawn_essential_handle();
		spawner.spawn("spec-msg-exchange", Some("spec-msg"), self.handler.run());
		spawner.spawn(
			"spec-msg-archiver",
			Some("spec-msg"),
			run_spec_msg_archiver(client.clone(), self.archive),
		);

		if !validator {
			return Ok(None);
		}

		let registry = Arc::new(PeerRegistry::default());
		for entry in source_peers {
			let (source, peer, address) = parse_source_peer(entry)?;
			log::info!("Speculative Messaging source peer for para {source}: {peer} @ {address}");
			network.add_known_address(peer, address);
			registry.add_peer(source, peer);
		}

		// DHT-based discovery of source peers over the relay-chain DHT (RFC-0008
		// `/paranode`), refreshing the same registry. The authoritative source
		// set + genesis comes from the runtime API (`source_discovery_info()`,
		// governance-set on-chain via `set_source_genesis` — so it tracks the
		// channel lifecycle), with the CLI `--spec-msg-source-genesis` values as
		// overrides for pinning/bootstrap. Runs regardless, since the on-chain
		// set is dynamic; a source pinned statically via `--spec-msg-source-peer`
		// simply keeps its registry entry (discovery never lists it).
		let mut overrides = HashMap::new();
		for entry in source_genesis {
			let (source, genesis_hash, fork_id) = parse_source_genesis(entry)?;
			log::info!(
				"Speculative Messaging DHT discovery override for para {source} (genesis {})",
				array_bytes::bytes2hex("0x", &genesis_hash),
			);
			overrides.insert(source, (genesis_hash, fork_id));
		}
		let discovery: Arc<dyn SourceDiscovery> = Arc::new(BootnodeSourceDiscovery::new(
			network.clone(),
			relay_chain_interface.clone(),
			relay_chain_network,
		));
		spawner.spawn(
			"spec-msg-discovery",
			Some("spec-msg"),
			run_spec_msg_discovery::<Block, _>(
				client.clone(),
				discovery,
				registry.clone(),
				overrides,
				DISCOVERY_REFRESH_INTERVAL,
			),
		);

		let pool = Arc::new(SpecMsgPool::default());
		let (events_tx, events_rx) = async_channel::bounded(EVENTS_CHANNEL_CAPACITY);
		spawner.spawn(
			"spec-msg-relay-monitor",
			Some("spec-msg"),
			run_relay_provides_monitor::<Block, _, _>(
				client.clone(),
				relay_chain_interface,
				events_tx,
			),
		);
		spawner.spawn(
			"spec-msg-fetcher",
			Some("spec-msg"),
			run_spec_msg_fetcher::<Block, _, _>(
				para_id,
				client.clone(),
				network,
				registry,
				pool.clone(),
				events_rx,
			),
		);

		let lift_assembler = lift_assembler(client, pool.clone());
		Ok(Some(SpecMsgDeps { pool, lift_assembler }))
	}
}

/// Parses one `--spec-msg-source-peer` value:
/// `<para-id>=<multiaddr>/p2p/<peer-id>`.
fn parse_source_peer(
	entry: &str,
) -> sc_service::error::Result<(ParaId, sc_network::PeerId, sc_network::Multiaddr)> {
	let error = |details: String| {
		sc_service::Error::Other(format!(
			"invalid --spec-msg-source-peer value `{entry}` \
			 (expected `<para-id>=<multiaddr>/p2p/<peer-id>`): {details}"
		))
	};
	let (para_id, address) =
		entry.split_once('=').ok_or_else(|| error("missing `=`".to_string()))?;
	let para_id: u32 = para_id.trim().parse().map_err(|e| error(format!("bad para id: {e}")))?;
	let (peer, address) = sc_network::config::parse_str_addr(address.trim())
		.map_err(|e| error(format!("bad multiaddr: {e}")))?;
	Ok((para_id.into(), peer, address))
}

/// Parses one `--spec-msg-source-genesis` value:
/// `<para-id>=<genesis-hash-hex>[/<fork-id>]`.
fn parse_source_genesis(
	entry: &str,
) -> sc_service::error::Result<(ParaId, Vec<u8>, Option<String>)> {
	let error = |details: String| {
		sc_service::Error::Other(format!(
			"invalid --spec-msg-source-genesis value `{entry}` \
			 (expected `<para-id>=<genesis-hash-hex>[/<fork-id>]`): {details}"
		))
	};
	let (para_id, rest) = entry.split_once('=').ok_or_else(|| error("missing `=`".to_string()))?;
	let para_id: u32 = para_id.trim().parse().map_err(|e| error(format!("bad para id: {e}")))?;
	let (genesis_hex, fork_id) = match rest.trim().split_once('/') {
		Some((hash, fork)) => (hash, (!fork.is_empty()).then(|| fork.to_string())),
		None => (rest.trim(), None),
	};
	let genesis_hex = genesis_hex.strip_prefix("0x").unwrap_or(genesis_hex);
	let genesis_hash = array_bytes::hex2bytes(genesis_hex)
		.map_err(|e| error(format!("bad genesis hash hex: {e:?}")))?;
	if genesis_hash.is_empty() {
		return Err(error("empty genesis hash".to_string()));
	}
	Ok((para_id.into(), genesis_hash, fork_id))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn source_peer_values_parse() {
		let (para, peer, addr) = parse_source_peer(
			"2000=/ip4/127.0.0.1/tcp/40100/ws\
			 /p2p/12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm",
		)
		.expect("valid value parses");
		assert_eq!(para, ParaId::from(2000));
		assert_eq!(peer.to_string(), "12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm");
		assert_eq!(addr.to_string(), "/ip4/127.0.0.1/tcp/40100/ws");

		for bad in [
			"2000",
			"abc=/ip4/127.0.0.1/tcp/1/ws/p2p/12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm",
			"2000=/ip4/127.0.0.1/tcp/40100/ws",
			"2000=nonsense",
		] {
			assert!(parse_source_peer(bad).is_err(), "`{bad}` must be rejected");
		}
	}

	#[test]
	fn source_genesis_values_parse() {
		// Bare genesis hash (0x-prefixed), no fork id.
		let (para, hash, fork) = parse_source_genesis("2000=0xaabbcc").expect("valid value parses");
		assert_eq!(para, ParaId::from(2000));
		assert_eq!(hash, vec![0xaa, 0xbb, 0xcc]);
		assert_eq!(fork, None);

		// Without `0x`, with a fork id.
		let (para, hash, fork) =
			parse_source_genesis("2001=aabb/mychain").expect("valid value parses");
		assert_eq!(para, ParaId::from(2001));
		assert_eq!(hash, vec![0xaa, 0xbb]);
		assert_eq!(fork.as_deref(), Some("mychain"));

		for bad in ["2000", "abc=0xaabb", "2000=0xzz", "2000=", "2000=0x"] {
			assert!(parse_source_genesis(bad).is_err(), "`{bad}` must be rejected");
		}
	}
}
