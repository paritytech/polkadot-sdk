// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Integration of the GRANDPA finality gadget into substrate.
//!
//! This crate is unstable and the API and usage may change.
//!
//! This crate provides a long-running future that produces finality notifications.
//!
//! # Usage
//!
//! First, create a block-import wrapper with the `block_import` function. The
//! GRANDPA worker needs to be linked together with this block import object, so
//! a `LinkHalf` is returned as well. All blocks imported (from network or
//! consensus or otherwise) must pass through this wrapper, otherwise consensus
//! is likely to break in unexpected ways.
//!
//! Next, use the `LinkHalf` and a local configuration to `run_grandpa_voter`.
//! This requires a `Network` implementation. The returned future should be
//! driven to completion and will finalize blocks in the background.
//!
//! # Changing authority sets
//!
//! The rough idea behind changing authority sets in GRANDPA is that at some point,
//! we obtain agreement for some maximum block height that the current set can
//! finalize, and once a block with that height is finalized the next set will
//! pick up finalization from there.
//!
//! Technically speaking, this would be implemented as a voting rule which says,
//! "if there is a signal for a change in N blocks in block B, only vote on
//! chains with length NUM(B) + N if they contain B". This conditional-inclusion
//! logic is complex to compute because it requires looking arbitrarily far
//! back in the chain.
//!
//! Instead, we keep track of a list of all signals we've seen so far (across
//! all forks), sorted ascending by the block number they would be applied at.
//! We never vote on chains with number higher than the earliest handoff block
//! number (this is num(signal) + N). When finalizing a block, we either apply
//! or prune any signaled changes based on whether the signaling block is
//! included in the newly-finalized chain.

use futures::prelude::*;
use futures::StreamExt;
use log::{debug, info};
use sc_client_api::{
	backend::{AuxStore, Backend},
	LockImportRun, BlockchainEvents, CallExecutor,
	ExecutionStrategy, Finalizer, TransactionFor, ExecutorProvider,
};
use sp_blockchain::{HeaderBackend, Error as ClientError, HeaderMetadata};
use parity_scale_codec::{Decode, Encode};
use prometheus_endpoint::{PrometheusError, Registry};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{NumberFor, Block as BlockT, DigestFor, Zero};
use sc_keystore::KeyStorePtr;
use sp_inherents::InherentDataProviders;
use sp_consensus::{SelectChain, BlockImport};
use sp_core::Pair;
use sp_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver};
use sc_telemetry::{telemetry, CONSENSUS_INFO, CONSENSUS_DEBUG};
use serde_json;

use sp_finality_tracker;

use finality_grandpa::Error as GrandpaError;
use finality_grandpa::{voter, BlockNumberOps, voter_set::VoterSet};

use std::{fmt, io};
use std::sync::Arc;
use std::time::Duration;
use std::pin::Pin;
use std::task::{Poll, Context};

// utility logging macro that takes as first argument a conditional to
// decide whether to log under debug or info level (useful to restrict
// logging under initial sync).
macro_rules! afg_log {
	($condition:expr, $($msg: expr),+ $(,)?) => {
		{
			let log_level = if $condition {
				log::Level::Debug
			} else {
				log::Level::Info
			};

			log::log!(target: "afg", log_level, $($msg),+);
		}
	};
}

mod authorities;
mod aux_schema;
mod communication;
mod consensus_changes;
mod environment;
mod finality_proof;
mod import;
mod justification;
mod light_import;
mod observer;
mod until_imported;
mod voting_rule;

pub use finality_proof::{FinalityProofProvider, StorageAndProofProvider};
pub use justification::GrandpaJustification;
pub use light_import::light_block_import;
pub use voting_rule::{
	BeforeBestBlockBy, ThreeQuartersOfTheUnfinalizedChain, VotingRule, VotingRulesBuilder
};

use aux_schema::PersistentData;
use environment::{Environment, VoterSetState};
use import::GrandpaBlockImport;
use until_imported::UntilGlobalMessageBlocksImported;
use communication::{NetworkBridge, Network as NetworkT};
use sp_finality_grandpa::{AuthorityList, AuthorityPair, AuthoritySignature, SetId};

// Re-export these two because it's just so damn convenient.
pub use sp_finality_grandpa::{AuthorityId, ScheduledChange};
use sp_api::ProvideRuntimeApi;
use std::marker::PhantomData;

#[cfg(test)]
mod tests;

/// A GRANDPA message for a substrate chain.
pub type Message<Block> = finality_grandpa::Message<<Block as BlockT>::Hash, NumberFor<Block>>;
/// A signed message.
pub type SignedMessage<Block> = finality_grandpa::SignedMessage<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;

/// A primary propose message for this chain's block type.
pub type PrimaryPropose<Block> = finality_grandpa::PrimaryPropose<<Block as BlockT>::Hash, NumberFor<Block>>;
/// A prevote message for this chain's block type.
pub type Prevote<Block> = finality_grandpa::Prevote<<Block as BlockT>::Hash, NumberFor<Block>>;
/// A precommit message for this chain's block type.
pub type Precommit<Block> = finality_grandpa::Precommit<<Block as BlockT>::Hash, NumberFor<Block>>;
/// A catch up message for this chain's block type.
pub type CatchUp<Block> = finality_grandpa::CatchUp<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;
/// A commit message for this chain's block type.
pub type Commit<Block> = finality_grandpa::Commit<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;
/// A compact commit message for this chain's block type.
pub type CompactCommit<Block> = finality_grandpa::CompactCommit<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;
/// A global communication input stream for commits and catch up messages. Not
/// exposed publicly, used internally to simplify types in the communication
/// layer.
type CommunicationIn<Block> = finality_grandpa::voter::CommunicationIn<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;

/// Global communication input stream for commits and catch up messages, with
/// the hash type not being derived from the block, useful for forcing the hash
/// to some type (e.g. `H256`) when the compiler can't do the inference.
type CommunicationInH<Block, H> = finality_grandpa::voter::CommunicationIn<
	H,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;

/// Global communication sink for commits with the hash type not being derived
/// from the block, useful for forcing the hash to some type (e.g. `H256`) when
/// the compiler can't do the inference.
type CommunicationOutH<Block, H> = finality_grandpa::voter::CommunicationOut<
	H,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;

/// Configuration for the GRANDPA service.
#[derive(Clone)]
pub struct Config {
	/// The expected duration for a message to be gossiped across the network.
	pub gossip_duration: Duration,
	/// Justification generation period (in blocks). GRANDPA will try to generate justifications
	/// at least every justification_period blocks. There are some other events which might cause
	/// justification generation.
	pub justification_period: u32,
	/// Whether the GRANDPA observer protocol is live on the network and thereby
	/// a full-node not running as a validator is running the GRANDPA observer
	/// protocol (we will only issue catch-up requests to authorities when the
	/// observer protocol is enabled).
	pub observer_enabled: bool,
	/// Whether the node is running as an authority (i.e. running the full GRANDPA protocol).
	pub is_authority: bool,
	/// Some local identifier of the voter.
	pub name: Option<String>,
	/// The keystore that manages the keys of this node.
	pub keystore: Option<sc_keystore::KeyStorePtr>,
}

impl Config {
	fn name(&self) -> &str {
		self.name.as_ref().map(|s| s.as_str()).unwrap_or("<unknown>")
	}
}

/// Errors that can occur while voting in GRANDPA.
#[derive(Debug)]
pub enum Error {
	/// An error within grandpa.
	Grandpa(GrandpaError),
	/// A network error.
	Network(String),
	/// A blockchain error.
	Blockchain(String),
	/// Could not complete a round on disk.
	Client(ClientError),
	/// An invariant has been violated (e.g. not finalizing pending change blocks in-order)
	Safety(String),
	/// A timer failed to fire.
	Timer(io::Error),
}

impl From<GrandpaError> for Error {
	fn from(e: GrandpaError) -> Self {
		Error::Grandpa(e)
	}
}

impl From<ClientError> for Error {
	fn from(e: ClientError) -> Self {
		Error::Client(e)
	}
}

/// Something which can determine if a block is known.
pub(crate) trait BlockStatus<Block: BlockT> {
	/// Return `Ok(Some(number))` or `Ok(None)` depending on whether the block
	/// is definitely known and has been imported.
	/// If an unexpected error occurs, return that.
	fn block_number(&self, hash: Block::Hash) -> Result<Option<NumberFor<Block>>, Error>;
}

impl<Block: BlockT, Client> BlockStatus<Block> for Arc<Client> where
	Client: HeaderBackend<Block>,
	NumberFor<Block>: BlockNumberOps,
{
	fn block_number(&self, hash: Block::Hash) -> Result<Option<NumberFor<Block>>, Error> {
		self.block_number_from_id(&BlockId::Hash(hash))
			.map_err(|e| Error::Blockchain(format!("{:?}", e)))
	}
}

/// A trait that includes all the client functionalities grandpa requires.
/// Ideally this would be a trait alias, we're not there yet.
/// tracking issue https://github.com/rust-lang/rust/issues/41517
pub trait ClientForGrandpa<Block, BE>:
	LockImportRun<Block, BE> + Finalizer<Block, BE> + AuxStore
	+ HeaderMetadata<Block, Error = sp_blockchain::Error> + HeaderBackend<Block>
	+ BlockchainEvents<Block> + ProvideRuntimeApi<Block> + ExecutorProvider<Block>
	+ BlockImport<Block, Transaction = TransactionFor<BE, Block>, Error = sp_consensus::Error>
	where
		BE: Backend<Block>,
		Block: BlockT,
{}

impl<Block, BE, T> ClientForGrandpa<Block, BE> for T
	where
		BE: Backend<Block>,
		Block: BlockT,
		T: LockImportRun<Block, BE> + Finalizer<Block, BE> + AuxStore
			+ HeaderMetadata<Block, Error = sp_blockchain::Error> + HeaderBackend<Block>
			+ BlockchainEvents<Block> + ProvideRuntimeApi<Block> + ExecutorProvider<Block>
			+ BlockImport<Block, Transaction = TransactionFor<BE, Block>, Error = sp_consensus::Error>,
{}

/// Something that one can ask to do a block sync request.
pub(crate) trait BlockSyncRequester<Block: BlockT> {
	/// Notifies the sync service to try and sync the given block from the given
	/// peers.
	///
	/// If the given vector of peers is empty then the underlying implementation
	/// should make a best effort to fetch the block from any peers it is
	/// connected to (NOTE: this assumption will change in the future #3629).
	fn set_sync_fork_request(&self, peers: Vec<sc_network::PeerId>, hash: Block::Hash, number: NumberFor<Block>);
}

impl<Block, Network> BlockSyncRequester<Block> for NetworkBridge<Block, Network> where
	Block: BlockT,
	Network: NetworkT<Block>,
{
	fn set_sync_fork_request(&self, peers: Vec<sc_network::PeerId>, hash: Block::Hash, number: NumberFor<Block>) {
		NetworkBridge::set_sync_fork_request(self, peers, hash, number)
	}
}

/// A new authority set along with the canonical block it changed at.
#[derive(Debug)]
pub(crate) struct NewAuthoritySet<H, N> {
	pub(crate) canon_number: N,
	pub(crate) canon_hash: H,
	pub(crate) set_id: SetId,
	pub(crate) authorities: AuthorityList,
}

/// Commands issued to the voter.
#[derive(Debug)]
pub(crate) enum VoterCommand<H, N> {
	/// Pause the voter for given reason.
	Pause(String),
	/// New authorities.
	ChangeAuthorities(NewAuthoritySet<H, N>)
}

impl<H, N> fmt::Display for VoterCommand<H, N> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			VoterCommand::Pause(ref reason) => write!(f, "Pausing voter: {}", reason),
			VoterCommand::ChangeAuthorities(_) => write!(f, "Changing authorities"),
		}
	}
}

/// Signals either an early exit of a voter or an error.
#[derive(Debug)]
pub(crate) enum CommandOrError<H, N> {
	/// An error occurred.
	Error(Error),
	/// A command to the voter.
	VoterCommand(VoterCommand<H, N>),
}

impl<H, N> From<Error> for CommandOrError<H, N> {
	fn from(e: Error) -> Self {
		CommandOrError::Error(e)
	}
}

impl<H, N> From<ClientError> for CommandOrError<H, N> {
	fn from(e: ClientError) -> Self {
		CommandOrError::Error(Error::Client(e))
	}
}

impl<H, N> From<finality_grandpa::Error> for CommandOrError<H, N> {
	fn from(e: finality_grandpa::Error) -> Self {
		CommandOrError::Error(Error::from(e))
	}
}

impl<H, N> From<VoterCommand<H, N>> for CommandOrError<H, N> {
	fn from(e: VoterCommand<H, N>) -> Self {
		CommandOrError::VoterCommand(e)
	}
}

impl<H: fmt::Debug, N: fmt::Debug> ::std::error::Error for CommandOrError<H, N> { }

impl<H, N> fmt::Display for CommandOrError<H, N> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			CommandOrError::Error(ref e) => write!(f, "{:?}", e),
			CommandOrError::VoterCommand(ref cmd) => write!(f, "{}", cmd),
		}
	}
}

pub struct LinkHalf<Block: BlockT, C, SC> {
	client: Arc<C>,
	select_chain: SC,
	persistent_data: PersistentData<Block>,
	voter_commands_rx: TracingUnboundedReceiver<VoterCommand<Block::Hash, NumberFor<Block>>>,
}

/// Provider for the Grandpa authority set configured on the genesis block.
pub trait GenesisAuthoritySetProvider<Block: BlockT> {
	/// Get the authority set at the genesis block.
	fn get(&self) -> Result<AuthorityList, ClientError>;
}

impl<Block: BlockT, E> GenesisAuthoritySetProvider<Block> for Arc<dyn ExecutorProvider<Block, Executor = E>>
	where E: CallExecutor<Block>,
{
	fn get(&self) -> Result<AuthorityList, ClientError> {
		// This implementation uses the Grandpa runtime API instead of reading directly from the
		// `GRANDPA_AUTHORITIES_KEY` as the data may have been migrated since the genesis block of
		// the chain, whereas the runtime API is backwards compatible.
		self.executor()
			.call(
				&BlockId::Number(Zero::zero()),
				"GrandpaApi_grandpa_authorities",
				&[],
				ExecutionStrategy::NativeElseWasm,
				None,
			)
			.and_then(|call_result| {
				Decode::decode(&mut &call_result[..])
					.map_err(|err| ClientError::CallResultDecode(
						"failed to decode GRANDPA authorities set proof".into(), err
					))
			})
	}
}

/// Make block importer and link half necessary to tie the background voter
/// to it.
pub fn block_import<BE, Block: BlockT, Client, SC>(
	client: Arc<Client>,
	genesis_authorities_provider: &dyn GenesisAuthoritySetProvider<Block>,
	select_chain: SC,
) -> Result<
	(
		GrandpaBlockImport<BE, Block, Client, SC>,
		LinkHalf<Block, Client, SC>,
	),
	ClientError,
>
where
	SC: SelectChain<Block>,
	BE: Backend<Block> + 'static,
	Client: ClientForGrandpa<Block, BE> + 'static,
{
	block_import_with_authority_set_hard_forks(
		client,
		genesis_authorities_provider,
		select_chain,
		Default::default(),
	)
}

/// Make block importer and link half necessary to tie the background voter to
/// it. A vector of authority set hard forks can be passed, any authority set
/// change signaled at the given block (either already signalled or in a further
/// block when importing it) will be replaced by a standard change with the
/// given static authorities.
pub fn block_import_with_authority_set_hard_forks<BE, Block: BlockT, Client, SC>(
	client: Arc<Client>,
	genesis_authorities_provider: &dyn GenesisAuthoritySetProvider<Block>,
	select_chain: SC,
	authority_set_hard_forks: Vec<(SetId, (Block::Hash, NumberFor<Block>), AuthorityList)>,
) -> Result<
	(
		GrandpaBlockImport<BE, Block, Client, SC>,
		LinkHalf<Block, Client, SC>,
	),
	ClientError,
>
where
	SC: SelectChain<Block>,
	BE: Backend<Block> + 'static,
	Client: ClientForGrandpa<Block, BE> + 'static,
{
	let chain_info = client.info();
	let genesis_hash = chain_info.genesis_hash;

	let persistent_data = aux_schema::load_persistent(
		&*client,
		genesis_hash,
		<NumberFor<Block>>::zero(),
		|| {
			let authorities = genesis_authorities_provider.get()?;
			telemetry!(CONSENSUS_DEBUG; "afg.loading_authorities";
				"authorities_len" => ?authorities.len()
			);
			Ok(authorities)
		}
	)?;

	let (voter_commands_tx, voter_commands_rx) = tracing_unbounded("mpsc_grandpa_voter_command");

	// create pending change objects with 0 delay and enacted on finality
	// (i.e. standard changes) for each authority set hard fork.
	let authority_set_hard_forks = authority_set_hard_forks
		.into_iter()
		.map(|(set_id, (hash, number), authorities)| {
			(
				set_id,
				authorities::PendingChange {
					next_authorities: authorities,
					delay: Zero::zero(),
					canon_hash: hash,
					canon_height: number,
					delay_kind: authorities::DelayKind::Finalized,
				},
			)
		})
		.collect();

	Ok((
		GrandpaBlockImport::new(
			client.clone(),
			select_chain.clone(),
			persistent_data.authority_set.clone(),
			voter_commands_tx,
			persistent_data.consensus_changes.clone(),
			authority_set_hard_forks,
		),
		LinkHalf {
			client,
			select_chain,
			persistent_data,
			voter_commands_rx,
		},
	))
}

fn global_communication<BE, Block: BlockT, C, N>(
	set_id: SetId,
	voters: &Arc<VoterSet<AuthorityId>>,
	client: Arc<C>,
	network: &NetworkBridge<Block, N>,
	keystore: &Option<KeyStorePtr>,
	metrics: Option<until_imported::Metrics>,
) -> (
	impl Stream<
		Item = Result<CommunicationInH<Block, Block::Hash>, CommandOrError<Block::Hash, NumberFor<Block>>>,
	>,
	impl Sink<
		CommunicationOutH<Block, Block::Hash>,
		Error = CommandOrError<Block::Hash, NumberFor<Block>>,
	> + Unpin,
) where
	BE: Backend<Block> + 'static,
	C: ClientForGrandpa<Block, BE> + 'static,
	N: NetworkT<Block>,
	NumberFor<Block>: BlockNumberOps,
{
	let is_voter = is_voter(voters, keystore).is_some();

	// verification stream
	let (global_in, global_out) = network.global_communication(
		communication::SetId(set_id),
		voters.clone(),
		is_voter,
	);

	// block commit and catch up messages until relevant blocks are imported.
	let global_in = UntilGlobalMessageBlocksImported::new(
		client.import_notification_stream(),
		network.clone(),
		client.clone(),
		global_in,
		"global",
		metrics,
	);

	let global_in = global_in.map_err(CommandOrError::from);
	let global_out = global_out.sink_map_err(CommandOrError::from);

	(global_in, global_out)
}

/// Register the finality tracker inherent data provider (which is used by
/// GRANDPA), if not registered already.
fn register_finality_tracker_inherent_data_provider<Block: BlockT, Client>(
	client: Arc<Client>,
	inherent_data_providers: &InherentDataProviders,
) -> Result<(), sp_consensus::Error> where
	Client: HeaderBackend<Block> + 'static,
{
	if !inherent_data_providers.has_provider(&sp_finality_tracker::INHERENT_IDENTIFIER) {
		inherent_data_providers
			.register_provider(sp_finality_tracker::InherentDataProvider::new(move || {
				#[allow(deprecated)]
				{
					let info = client.info();
					telemetry!(CONSENSUS_INFO; "afg.finalized";
						"finalized_number" => ?info.finalized_number,
						"finalized_hash" => ?info.finalized_hash,
					);
					Ok(info.finalized_number)
				}
			}))
			.map_err(|err| sp_consensus::Error::InherentData(err.into()))
	} else {
		Ok(())
	}
}

/// Parameters used to run Grandpa.
pub struct GrandpaParams<Block: BlockT, C, N, SC, VR> {
	/// Configuration for the GRANDPA service.
	pub config: Config,
	/// A link to the block import worker.
	pub link: LinkHalf<Block, C, SC>,
	/// The Network instance.
	pub network: N,
	/// The inherent data providers.
	pub inherent_data_providers: InherentDataProviders,
	/// If supplied, can be used to hook on telemetry connection established events.
	pub telemetry_on_connect: Option<TracingUnboundedReceiver<()>>,
	/// A voting rule used to potentially restrict target votes.
	pub voting_rule: VR,
	/// The prometheus metrics registry.
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
}

/// Run a GRANDPA voter as a task. Provide configuration and a link to a
/// block import worker that has already been instantiated with `block_import`.
pub fn run_grandpa_voter<Block: BlockT, BE: 'static, C, N, SC, VR>(
	grandpa_params: GrandpaParams<Block, C, N, SC, VR>,
) -> sp_blockchain::Result<impl Future<Output = ()> + Unpin + Send + 'static> where
	Block::Hash: Ord,
	BE: Backend<Block> + 'static,
	N: NetworkT<Block> + Send + Sync + Clone + 'static,
	SC: SelectChain<Block> + 'static,
	VR: VotingRule<Block, C> + Clone + 'static,
	NumberFor<Block>: BlockNumberOps,
	DigestFor<Block>: Encode,
	C: ClientForGrandpa<Block, BE> + 'static,
{
	let GrandpaParams {
		mut config,
		link,
		network,
		inherent_data_providers,
		telemetry_on_connect,
		voting_rule,
		prometheus_registry,
	} = grandpa_params;

	// NOTE: we have recently removed `run_grandpa_observer` from the public
	// API, I felt it is easier to just ignore this field rather than removing
	// it from the config temporarily. This should be removed after #5013 is
	// fixed and we re-add the observer to the public API.
	config.observer_enabled = false;

	let LinkHalf {
		client,
		select_chain,
		persistent_data,
		voter_commands_rx,
	} = link;

	let network = NetworkBridge::new(
		network,
		config.clone(),
		persistent_data.set_state.clone(),
		prometheus_registry.as_ref(),
	);

	register_finality_tracker_inherent_data_provider(client.clone(), &inherent_data_providers)?;

	let conf = config.clone();
	let telemetry_task = if let Some(telemetry_on_connect) = telemetry_on_connect {
		let authorities = persistent_data.authority_set.clone();
		let events = telemetry_on_connect
			.for_each(move |_| {
				let curr = authorities.current_authorities();
				let mut auths = curr.voters().into_iter().map(|(p, _)| p);
				let maybe_authority_id = authority_id(&mut auths, &conf.keystore)
					.unwrap_or(Default::default());

				telemetry!(CONSENSUS_INFO; "afg.authority_set";
					"authority_id" => maybe_authority_id.to_string(),
					"authority_set_id" => ?authorities.set_id(),
					"authorities" => {
						let authorities: Vec<String> = curr.voters()
							.iter().map(|(id, _)| id.to_string()).collect();
						serde_json::to_string(&authorities)
							.expect("authorities is always at least an empty vector; elements are always of type string")
					}
				);
				future::ready(())
			});
		future::Either::Left(events)
	} else {
		future::Either::Right(future::pending())
	};

	let voter_work = VoterWork::new(
		client,
		config,
		network,
		select_chain,
		voting_rule,
		persistent_data,
		voter_commands_rx,
		prometheus_registry,
	);

	let voter_work = voter_work
		.map(|_| ());

	// Make sure that `telemetry_task` doesn't accidentally finish and kill grandpa.
	let telemetry_task = telemetry_task
		.then(|_| future::pending::<()>());

	Ok(future::select(voter_work, telemetry_task).map(drop))
}

struct Metrics {
	environment: environment::Metrics,
	until_imported: until_imported::Metrics,
}

impl Metrics {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Metrics {
			environment: environment::Metrics::register(registry)?,
			until_imported: until_imported::Metrics::register(registry)?,
		})
	}
}

/// Future that powers the voter.
#[must_use]
struct VoterWork<B, Block: BlockT, C, N: NetworkT<Block>, SC, VR> {
	voter: Pin<Box<dyn Future<Output = Result<(), CommandOrError<Block::Hash, NumberFor<Block>>>> + Send>>,
	env: Arc<Environment<B, Block, C, N, SC, VR>>,
	voter_commands_rx: TracingUnboundedReceiver<VoterCommand<Block::Hash, NumberFor<Block>>>,
	network: NetworkBridge<Block, N>,

	/// Prometheus metrics.
	metrics: Option<Metrics>,
}

impl<B, Block, C, N, SC, VR> VoterWork<B, Block, C, N, SC, VR>
where
	Block: BlockT,
	B: Backend<Block> + 'static,
	C: ClientForGrandpa<Block, B> + 'static,
	N: NetworkT<Block> + Sync,
	NumberFor<Block>: BlockNumberOps,
	SC: SelectChain<Block> + 'static,
	VR: VotingRule<Block, C> + Clone + 'static,
{
	fn new(
		client: Arc<C>,
		config: Config,
		network: NetworkBridge<Block, N>,
		select_chain: SC,
		voting_rule: VR,
		persistent_data: PersistentData<Block>,
		voter_commands_rx: TracingUnboundedReceiver<VoterCommand<Block::Hash, NumberFor<Block>>>,
		prometheus_registry: Option<prometheus_endpoint::Registry>,
	) -> Self {
		let metrics = match prometheus_registry.as_ref().map(Metrics::register) {
			Some(Ok(metrics)) => Some(metrics),
			Some(Err(e)) => {
				debug!(target: "afg", "Failed to register metrics: {:?}", e);
				None
			}
			None => None,
		};

		let voters = persistent_data.authority_set.current_authorities();
		let env = Arc::new(Environment {
			client,
			select_chain,
			voting_rule,
			voters: Arc::new(voters),
			config,
			network: network.clone(),
			set_id: persistent_data.authority_set.set_id(),
			authority_set: persistent_data.authority_set.clone(),
			consensus_changes: persistent_data.consensus_changes.clone(),
			voter_set_state: persistent_data.set_state.clone(),
			metrics: metrics.as_ref().map(|m| m.environment.clone()),
			_phantom: PhantomData,
		});

		let mut work = VoterWork {
			// `voter` is set to a temporary value and replaced below when
			// calling `rebuild_voter`.
			voter: Box::pin(future::pending()),
			env,
			voter_commands_rx,
			network,
			metrics,
		};
		work.rebuild_voter();
		work
	}

	/// Rebuilds the `self.voter` field using the current authority set
	/// state. This method should be called when we know that the authority set
	/// has changed (e.g. as signalled by a voter command).
	fn rebuild_voter(&mut self) {
		debug!(target: "afg", "{}: Starting new voter with set ID {}", self.env.config.name(), self.env.set_id);

		let authority_id = is_voter(&self.env.voters, &self.env.config.keystore)
			.map(|ap| ap.public())
			.unwrap_or(Default::default());

		telemetry!(CONSENSUS_DEBUG; "afg.starting_new_voter";
			"name" => ?self.env.config.name(),
			"set_id" => ?self.env.set_id,
			"authority_id" => authority_id.to_string(),
		);

		let chain_info = self.env.client.info();
		telemetry!(CONSENSUS_INFO; "afg.authority_set";
			"number" => ?chain_info.finalized_number,
			"hash" => ?chain_info.finalized_hash,
			"authority_id" => authority_id.to_string(),
			"authority_set_id" => ?self.env.set_id,
			"authorities" => {
				let authorities: Vec<String> = self.env.voters.voters()
					.iter().map(|(id, _)| id.to_string()).collect();
				serde_json::to_string(&authorities)
					.expect("authorities is always at least an empty vector; elements are always of type string")
			},
		);

		match &*self.env.voter_set_state.read() {
			VoterSetState::Live { completed_rounds, .. } => {
				let last_finalized = (
					chain_info.finalized_hash,
					chain_info.finalized_number,
				);

				let global_comms = global_communication(
					self.env.set_id,
					&self.env.voters,
					self.env.client.clone(),
					&self.env.network,
					&self.env.config.keystore,
					self.metrics.as_ref().map(|m| m.until_imported.clone()),
				);

				let last_completed_round = completed_rounds.last();

				let voter = voter::Voter::new(
					self.env.clone(),
					(*self.env.voters).clone(),
					global_comms,
					last_completed_round.number,
					last_completed_round.votes.clone(),
					last_completed_round.base.clone(),
					last_finalized,
				);

				self.voter = Box::pin(voter);
			},
			VoterSetState::Paused { .. } =>
				self.voter = Box::pin(future::pending()),
		};
	}

	fn handle_voter_command(
		&mut self,
		command: VoterCommand<Block::Hash, NumberFor<Block>>
	) -> Result<(), Error> {
		match command {
			VoterCommand::ChangeAuthorities(new) => {
				let voters: Vec<String> = new.authorities.iter().map(move |(a, _)| {
					format!("{}", a)
				}).collect();
				telemetry!(CONSENSUS_INFO; "afg.voter_command_change_authorities";
					"number" => ?new.canon_number,
					"hash" => ?new.canon_hash,
					"voters" => ?voters,
					"set_id" => ?new.set_id,
				);

				self.env.update_voter_set_state(|_| {
					// start the new authority set using the block where the
					// set changed (not where the signal happened!) as the base.
					let set_state = VoterSetState::live(
						new.set_id,
						&*self.env.authority_set.inner().read(),
						(new.canon_hash, new.canon_number),
					);

					aux_schema::write_voter_set_state(&*self.env.client, &set_state)?;
					Ok(Some(set_state))
				})?;

				self.env = Arc::new(Environment {
					voters: Arc::new(new.authorities.into_iter().collect()),
					set_id: new.set_id,
					voter_set_state: self.env.voter_set_state.clone(),
					// Fields below are simply transferred and not updated.
					client: self.env.client.clone(),
					select_chain: self.env.select_chain.clone(),
					config: self.env.config.clone(),
					authority_set: self.env.authority_set.clone(),
					consensus_changes: self.env.consensus_changes.clone(),
					network: self.env.network.clone(),
					voting_rule: self.env.voting_rule.clone(),
					metrics: self.env.metrics.clone(),
					_phantom: PhantomData,
				});

				self.rebuild_voter();
				Ok(())
			}
			VoterCommand::Pause(reason) => {
				info!(target: "afg", "Pausing old validator set: {}", reason);

				// not racing because old voter is shut down.
				self.env.update_voter_set_state(|voter_set_state| {
					let completed_rounds = voter_set_state.completed_rounds();
					let set_state = VoterSetState::Paused { completed_rounds };

					aux_schema::write_voter_set_state(&*self.env.client, &set_state)?;
					Ok(Some(set_state))
				})?;

				self.rebuild_voter();
				Ok(())
			}
		}
	}
}

impl<B, Block, C, N, SC, VR> Future for VoterWork<B, Block, C, N, SC, VR>
where
	Block: BlockT,
	B: Backend<Block> + 'static,
	N: NetworkT<Block> + Sync,
	NumberFor<Block>: BlockNumberOps,
	SC: SelectChain<Block> + 'static,
	C: ClientForGrandpa<Block, B> + 'static,
	VR: VotingRule<Block, C> + Clone + 'static,
{
	type Output = Result<(), Error>;

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		match Future::poll(Pin::new(&mut self.voter), cx) {
			Poll::Pending => {}
			Poll::Ready(Ok(())) => {
				// voters don't conclude naturally
				return Poll::Ready(Err(Error::Safety("GRANDPA voter has concluded.".into())))
			}
			Poll::Ready(Err(CommandOrError::Error(e))) => {
				// return inner observer error
				return Poll::Ready(Err(e))
			}
			Poll::Ready(Err(CommandOrError::VoterCommand(command))) => {
				// some command issued internally
				self.handle_voter_command(command)?;
				cx.waker().wake_by_ref();
			}
		}

		match Stream::poll_next(Pin::new(&mut self.voter_commands_rx), cx) {
			Poll::Pending => {}
			Poll::Ready(None) => {
				// the `voter_commands_rx` stream should never conclude since it's never closed.
				return Poll::Ready(Ok(()))
			}
			Poll::Ready(Some(command)) => {
				// some command issued externally
				self.handle_voter_command(command)?;
				cx.waker().wake_by_ref();
			}
		}

		Future::poll(Pin::new(&mut self.network), cx)
	}
}

/// When GRANDPA is not initialized we still need to register the finality
/// tracker inherent provider which might be expected by the runtime for block
/// authoring. Additionally, we register a gossip message validator that
/// discards all GRANDPA messages (otherwise, we end up banning nodes that send
/// us a `Neighbor` message, since there is no registered gossip validator for
/// the engine id defined in the message.)
pub fn setup_disabled_grandpa<Block: BlockT, Client, N>(
	client: Arc<Client>,
	inherent_data_providers: &InherentDataProviders,
	network: N,
) -> Result<(), sp_consensus::Error> where
	N: NetworkT<Block> + Send + Clone + 'static,
	Client: HeaderBackend<Block> + 'static,
{
	register_finality_tracker_inherent_data_provider(
		client,
		inherent_data_providers,
	)?;

	// We register the GRANDPA protocol so that we don't consider it an anomaly
	// to receive GRANDPA messages on the network. We don't process the
	// messages.
	network.register_notifications_protocol(
		communication::GRANDPA_ENGINE_ID,
		From::from(communication::GRANDPA_PROTOCOL_NAME),
	);

	Ok(())
}

/// Checks if this node is a voter in the given voter set.
///
/// Returns the key pair of the node that is being used in the current voter set or `None`.
fn is_voter(
	voters: &Arc<VoterSet<AuthorityId>>,
	keystore: &Option<KeyStorePtr>,
) -> Option<AuthorityPair> {
	match keystore {
		Some(keystore) => voters.voters().iter()
			.find_map(|(p, _)| keystore.read().key_pair::<AuthorityPair>(&p).ok()),
		None => None,
	}
}

/// Returns the authority id of this node, if available.
fn authority_id<'a, I>(
	authorities: &mut I,
	keystore: &Option<KeyStorePtr>,
) -> Option<AuthorityId> where
	I: Iterator<Item = &'a AuthorityId>,
{
	match keystore {
		Some(keystore) => {
			authorities
				.find_map(|p| {
					keystore.read().key_pair::<AuthorityPair>(&p)
						.ok()
						.map(|ap| ap.public())
				})
		}
		None => None,
	}
}
