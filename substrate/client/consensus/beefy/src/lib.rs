// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
	communication::{
		notification::{
			BeefyBestBlockSender, BeefyBestBlockStream, BeefyVersionedFinalityProofSender,
			BeefyVersionedFinalityProofStream,
		},
		peers::KnownPeers,
		request_response::{
			outgoing_requests_engine::OnDemandJustificationsEngine, BeefyJustifsRequestHandler,
		},
	},
	error::Error,
	import::BeefyBlockImport,
	metrics::register_metrics,
};
use futures::{stream::Fuse, StreamExt};
use log::{debug, error, info, warn};
use parking_lot::Mutex;
use prometheus::Registry;
use sc_client_api::{Backend, BlockBackend, BlockchainEvents, FinalityNotifications, Finalizer};
use sc_consensus::BlockImport;
use sc_network::{NetworkRequest, NotificationService, ProtocolName};
use sc_network_gossip::{GossipEngine, Network as GossipNetwork, Syncing as GossipSyncing};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{
	Backend as BlockchainBackend, Error as ClientError, HeaderBackend, Result as ClientResult,
};
use sp_consensus::{Error as ConsensusError, SyncOracle};
use sp_consensus_beefy::{
	ecdsa_crypto::AuthorityId, BeefyApi, MmrRootHash, PayloadProvider, ValidatorSet,
};
use sp_keystore::KeystorePtr;
use sp_mmr_primitives::MmrApi;
use sp_runtime::traits::{Block, Header as HeaderT, NumberFor, Zero};
use std::{collections::BTreeMap, marker::PhantomData, sync::Arc, time::Duration};

mod aux_schema;
mod error;
mod keystore;
mod metrics;
mod round;
mod worker;

pub mod communication;
pub mod import;
pub mod justification;

pub use communication::beefy_protocol_name::{
	gossip_protocol_name, justifications_protocol_name as justifs_protocol_name,
};

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "beefy";

const HEADER_SYNC_DELAY: Duration = Duration::from_secs(60);

/// A convenience BEEFY client trait that defines all the type bounds a BEEFY client
/// has to satisfy. Ideally that should actually be a trait alias. Unfortunately as
/// of today, Rust does not allow a type alias to be used as a trait bound. Tracking
/// issue is <https://github.com/rust-lang/rust/issues/41517>.
pub trait Client<B, BE>:
	BlockchainEvents<B> + HeaderBackend<B> + Finalizer<B, BE> + Send + Sync
where
	B: Block,
	BE: Backend<B>,
{
	// empty
}

impl<B, BE, T> Client<B, BE> for T
where
	B: Block,
	BE: Backend<B>,
	T: BlockchainEvents<B>
		+ HeaderBackend<B>
		+ Finalizer<B, BE>
		+ ProvideRuntimeApi<B>
		+ Send
		+ Sync,
{
	// empty
}

/// Links between the block importer, the background voter and the RPC layer,
/// to be used by the voter.
#[derive(Clone)]
pub struct BeefyVoterLinks<B: Block> {
	// BlockImport -> Voter links
	/// Stream of BEEFY signed commitments from block import to voter.
	pub from_block_import_justif_stream: BeefyVersionedFinalityProofStream<B>,

	// Voter -> RPC links
	/// Sends BEEFY signed commitments from voter to RPC.
	pub to_rpc_justif_sender: BeefyVersionedFinalityProofSender<B>,
	/// Sends BEEFY best block hashes from voter to RPC.
	pub to_rpc_best_block_sender: BeefyBestBlockSender<B>,
}

/// Links used by the BEEFY RPC layer, from the BEEFY background voter.
#[derive(Clone)]
pub struct BeefyRPCLinks<B: Block> {
	/// Stream of signed commitments coming from the voter.
	pub from_voter_justif_stream: BeefyVersionedFinalityProofStream<B>,
	/// Stream of BEEFY best block hashes coming from the voter.
	pub from_voter_best_beefy_stream: BeefyBestBlockStream<B>,
}

/// Make block importer and link half necessary to tie the background voter to it.
pub fn beefy_block_import_and_links<B, BE, RuntimeApi, I>(
	wrapped_block_import: I,
	backend: Arc<BE>,
	runtime: Arc<RuntimeApi>,
	prometheus_registry: Option<Registry>,
) -> (BeefyBlockImport<B, BE, RuntimeApi, I>, BeefyVoterLinks<B>, BeefyRPCLinks<B>)
where
	B: Block,
	BE: Backend<B>,
	I: BlockImport<B, Error = ConsensusError> + Send + Sync,
	RuntimeApi: ProvideRuntimeApi<B> + Send + Sync,
	RuntimeApi::Api: BeefyApi<B, AuthorityId>,
{
	// Voter -> RPC links
	let (to_rpc_justif_sender, from_voter_justif_stream) =
		BeefyVersionedFinalityProofStream::<B>::channel();
	let (to_rpc_best_block_sender, from_voter_best_beefy_stream) =
		BeefyBestBlockStream::<B>::channel();

	// BlockImport -> Voter links
	let (to_voter_justif_sender, from_block_import_justif_stream) =
		BeefyVersionedFinalityProofStream::<B>::channel();
	let metrics = register_metrics(prometheus_registry);

	// BlockImport
	let import = BeefyBlockImport::new(
		backend,
		runtime,
		wrapped_block_import,
		to_voter_justif_sender,
		metrics,
	);
	let voter_links = BeefyVoterLinks {
		from_block_import_justif_stream,
		to_rpc_justif_sender,
		to_rpc_best_block_sender,
	};
	let rpc_links = BeefyRPCLinks { from_voter_best_beefy_stream, from_voter_justif_stream };

	(import, voter_links, rpc_links)
}

/// BEEFY gadget network parameters.
pub struct BeefyNetworkParams<B: Block, N, S> {
	/// Network implementing gossip, requests and sync-oracle.
	pub network: Arc<N>,
	/// Syncing service implementing a sync oracle and an event stream for peers.
	pub sync: Arc<S>,
	/// Handle for receiving notification events.
	pub notification_service: Box<dyn NotificationService>,
	/// Chain specific BEEFY gossip protocol name. See
	/// [`communication::beefy_protocol_name::gossip_protocol_name`].
	pub gossip_protocol_name: ProtocolName,
	/// Chain specific BEEFY on-demand justifications protocol name. See
	/// [`communication::beefy_protocol_name::justifications_protocol_name`].
	pub justifications_protocol_name: ProtocolName,

	pub _phantom: PhantomData<B>,
}

/// BEEFY gadget initialization parameters.
pub struct BeefyParams<B: Block, BE, C, N, P, R, S> {
	/// BEEFY client
	pub client: Arc<C>,
	/// Client Backend
	pub backend: Arc<BE>,
	/// BEEFY Payload provider
	pub payload_provider: P,
	/// Runtime Api Provider
	pub runtime: Arc<R>,
	/// Local key store
	pub key_store: Option<KeystorePtr>,
	/// BEEFY voter network params
	pub network_params: BeefyNetworkParams<B, N, S>,
	/// Minimal delta between blocks, BEEFY should vote for
	pub min_block_delta: u32,
	/// Prometheus metric registry
	pub prometheus_registry: Option<Registry>,
	/// Links between the block importer, the background voter and the RPC layer.
	pub links: BeefyVoterLinks<B>,
	/// Handler for incoming BEEFY justifications requests from a remote peer.
	pub on_demand_justifications_handler: BeefyJustifsRequestHandler<B, C>,
}

/// Start the BEEFY gadget.
///
/// This is a thin shim around running and awaiting a BEEFY worker.
pub async fn start_beefy_gadget<B, BE, C, N, P, R, S>(
	beefy_params: BeefyParams<B, BE, C, N, P, R, S>,
) where
	B: Block,
	BE: Backend<B>,
	C: Client<B, BE> + BlockBackend<B>,
	P: PayloadProvider<B> + Clone,
	R: ProvideRuntimeApi<B>,
	R::Api: BeefyApi<B, AuthorityId> + MmrApi<B, MmrRootHash, NumberFor<B>>,
	N: GossipNetwork<B> + NetworkRequest + Send + Sync + 'static,
	S: GossipSyncing<B> + SyncOracle + 'static,
{
	let BeefyParams {
		client,
		backend,
		payload_provider,
		runtime,
		key_store,
		network_params,
		min_block_delta,
		prometheus_registry,
		links,
		mut on_demand_justifications_handler,
	} = beefy_params;

	let BeefyNetworkParams {
		network,
		sync,
		notification_service,
		gossip_protocol_name,
		justifications_protocol_name,
		..
	} = network_params;

	let metrics = register_metrics(prometheus_registry.clone());

	// Subscribe to finality notifications and justifications before waiting for runtime pallet and
	// reuse the streams, so we don't miss notifications while waiting for pallet to be available.
	let mut finality_notifications = client.finality_notification_stream().fuse();
	let mut block_import_justif = links.from_block_import_justif_stream.subscribe(100_000).fuse();

	let known_peers = Arc::new(Mutex::new(KnownPeers::new()));
	// Default votes filter is to discard everything.
	// Validator is updated later with correct starting round and set id.
	let (gossip_validator, gossip_report_stream) =
		communication::gossip::GossipValidator::new(known_peers.clone());
	let gossip_validator = Arc::new(gossip_validator);
	let gossip_engine = GossipEngine::new(
		network.clone(),
		sync.clone(),
		notification_service,
		gossip_protocol_name.clone(),
		gossip_validator.clone(),
		None,
	);

	// The `GossipValidator` adds and removes known peers based on valid votes and network
	// events.
	let on_demand_justifications = OnDemandJustificationsEngine::new(
		network.clone(),
		justifications_protocol_name.clone(),
		known_peers,
		prometheus_registry.clone(),
	);
	let mut beefy_comms = worker::BeefyComms {
		gossip_engine,
		gossip_validator,
		gossip_report_stream,
		on_demand_justifications,
	};

	// We re-create and re-run the worker in this loop in order to quickly reinit and resume after
	// select recoverable errors.
	loop {
		// Wait for BEEFY pallet to be active before starting voter.
		let (beefy_genesis, best_grandpa) = match wait_for_runtime_pallet(
			&*runtime,
			&mut beefy_comms.gossip_engine,
			&mut finality_notifications,
		)
		.await
		{
			Ok(res) => res,
			Err(e) => {
				error!(target: LOG_TARGET, "Error: {:?}. Terminating.", e);
				return
			},
		};

		let mut worker_base = worker::BeefyWorkerBase {
			backend: backend.clone(),
			runtime: runtime.clone(),
			key_store: key_store.clone().into(),
			metrics: metrics.clone(),
			_phantom: Default::default(),
		};

		let persisted_state = match worker_base
			.load_or_init_state(beefy_genesis, best_grandpa, min_block_delta)
			.await
		{
			Ok(state) => state,
			Err(e) => {
				error!(target: LOG_TARGET, "Error: {:?}. Terminating.", e);
				return
			},
		};
		// Update the gossip validator with the right starting round and set id.
		if let Err(e) = persisted_state
			.gossip_filter_config()
			.map(|f| beefy_comms.gossip_validator.update_filter(f))
		{
			error!(target: LOG_TARGET, "Error: {:?}. Terminating.", e);
			return
		}

		let worker = worker::BeefyWorker {
			base: worker_base,
			payload_provider: payload_provider.clone(),
			sync: sync.clone(),
			comms: beefy_comms,
			links: links.clone(),
			pending_justifications: BTreeMap::new(),
			persisted_state,
		};

		match futures::future::select(
			Box::pin(worker.run(&mut block_import_justif, &mut finality_notifications)),
			Box::pin(on_demand_justifications_handler.run()),
		)
		.await
		{
			// On `ConsensusReset` error, just reinit and restart voter.
			futures::future::Either::Left(((error::Error::ConsensusReset, reuse_comms), _)) => {
				error!(target: LOG_TARGET, "🥩 Error: {:?}. Restarting voter.", error::Error::ConsensusReset);
				beefy_comms = reuse_comms;
				continue
			},
			// On other errors, bring down / finish the task.
			futures::future::Either::Left(((worker_err, _), _)) =>
				error!(target: LOG_TARGET, "🥩 Error: {:?}. Terminating.", worker_err),
			futures::future::Either::Right((odj_handler_err, _)) =>
				error!(target: LOG_TARGET, "🥩 Error: {:?}. Terminating.", odj_handler_err),
		};
		return
	}
}

/// Waits until the parent header of `current` is available and returns it.
///
/// When the node uses GRANDPA warp sync it initially downloads only the mandatory GRANDPA headers.
/// The rest of the headers (gap sync) are lazily downloaded later. But the BEEFY voter also needs
/// the headers in range `[beefy_genesis..=best_grandpa]` to be available. This helper method
/// enables us to wait until these headers have been synced.
async fn wait_for_parent_header<B, BC>(
	blockchain: &BC,
	current: <B as Block>::Header,
	delay: Duration,
) -> Result<<B as Block>::Header, Error>
where
	B: Block,
	BC: BlockchainBackend<B>,
{
	if *current.number() == Zero::zero() {
		let msg = format!("header {} is Genesis, there is no parent for it", current.hash());
		warn!(target: LOG_TARGET, "{}", msg);
		return Err(Error::Backend(msg));
	}
	loop {
		match blockchain
			.header(*current.parent_hash())
			.map_err(|e| Error::Backend(e.to_string()))?
		{
			Some(parent) => return Ok(parent),
			None => {
				info!(
					target: LOG_TARGET,
					"🥩 Parent of header number {} not found. \
					BEEFY gadget waiting for header sync to finish ...",
					current.number()
				);
				tokio::time::sleep(delay).await;
			},
		}
	}
}

/// Wait for BEEFY runtime pallet to be available, return active validator set.
/// Should be called only once during worker initialization.
async fn wait_for_runtime_pallet<B, R>(
	runtime: &R,
	mut gossip_engine: &mut GossipEngine<B>,
	finality: &mut Fuse<FinalityNotifications<B>>,
) -> ClientResult<(NumberFor<B>, <B as Block>::Header)>
where
	B: Block,
	R: ProvideRuntimeApi<B>,
	R::Api: BeefyApi<B, AuthorityId>,
{
	info!(target: LOG_TARGET, "🥩 BEEFY gadget waiting for BEEFY pallet to become available...");
	loop {
		futures::select! {
			notif = finality.next() => {
				let notif = match notif {
					Some(notif) => notif,
					None => break
				};
				let at = notif.header.hash();
				if let Some(start) = runtime.runtime_api().beefy_genesis(at).ok().flatten() {
					if *notif.header.number() >= start {
						// Beefy pallet available, return header for best grandpa at the time.
						info!(
							target: LOG_TARGET,
							"🥩 BEEFY pallet available: block {:?} beefy genesis {:?}",
							notif.header.number(), start
						);
						return Ok((start, notif.header))
					}
				}
			},
			_ = gossip_engine => {
				break
			}
		}
	}
	let err_msg = "🥩 Gossip engine has unexpectedly terminated.".into();
	error!(target: LOG_TARGET, "{}", err_msg);
	Err(ClientError::Backend(err_msg))
}

/// Provides validator set active `at_header`. It tries to get it from state, otherwise falls
/// back to walk up the chain looking the validator set enactment in header digests.
///
/// Note: function will `async::sleep()` when walking back the chain if some needed header hasn't
/// been synced yet (as it happens when warp syncing when headers are synced in the background).
async fn expect_validator_set<B, BE, R>(
	runtime: &R,
	backend: &BE,
	at_header: &B::Header,
) -> Result<ValidatorSet<AuthorityId>, Error>
where
	B: Block,
	BE: Backend<B>,
	R: ProvideRuntimeApi<B>,
	R::Api: BeefyApi<B, AuthorityId>,
{
	let blockchain = backend.blockchain();
	// Walk up the chain looking for the validator set active at 'at_header'. Process both state and
	// header digests.
	debug!(
		target: LOG_TARGET,
		"🥩 Trying to find validator set active at header(number {:?}, hash {:?})",
		at_header.number(),
		at_header.hash()
	);
	let mut header = at_header.clone();
	loop {
		debug!(target: LOG_TARGET, "🥩 Looking for auth set change at block number: {:?}", *header.number());
		if let Ok(Some(active)) = runtime.runtime_api().validator_set(header.hash()) {
			return Ok(active)
		} else {
			match worker::find_authorities_change::<B>(&header) {
				Some(active) => return Ok(active),
				// Move up the chain. Ultimately we'll get it from chain genesis state, or error out
				// there.
				None =>
					header = wait_for_parent_header(blockchain, header, HEADER_SYNC_DELAY)
						.await
						.map_err(|e| Error::Backend(e.to_string()))?,
			}
		}
	}
}
