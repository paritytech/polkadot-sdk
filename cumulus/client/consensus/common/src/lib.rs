// Copyright 2019 Parity Technologies (UK) Ltd.
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

use sc_client_api::{
	Backend, BlockBackend, BlockImportNotification, BlockchainEvents, Finalizer, UsageProvider,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, BlockStatus, Error as ConsensusError,
	ForkChoiceStrategy, SelectChain as SelectChainT,
};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use polkadot_primitives::v1::{
	Block as PBlock, Hash as PHash, Id as ParaId, OccupiedCoreAssumption, ParachainHost,
	PersistedValidationData,
};

use codec::Decode;
use futures::{future, select, FutureExt, Stream, StreamExt};

use std::{marker::PhantomData, sync::Arc};

/// Errors that can occur while following the polkadot relay-chain.
#[derive(Debug)]
pub enum Error {
	/// An underlying client error.
	Client(ClientError),
	/// Head data returned was not for our parachain.
	InvalidHeadData,
}

/// Helper for the relay chain client. This is expected to be a lightweight handle like an `Arc`.
pub trait RelaychainClient: Clone + 'static {
	/// The error type for interacting with the Polkadot client.
	type Error: std::fmt::Debug + Send;

	/// A stream that yields head-data for a parachain.
	type HeadStream: Stream<Item = Vec<u8>> + Send + Unpin;

	/// Get a stream of new best heads for the given parachain.
	fn new_best_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream>;

	/// Get a stream of finalized heads for the given parachain.
	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream>;

	/// Returns the parachain head for the given `para_id` at the given block id.
	fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> ClientResult<Option<Vec<u8>>>;
}

/// Follow the finalized head of the given parachain.
///
/// For every finalized block of the relay chain, it will get the included parachain header
/// corresponding to `para_id` and will finalize it in the parachain.
async fn follow_finalized_head<P, Block, B, R>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
) -> ClientResult<()>
where
	Block: BlockT,
	P: Finalizer<Block, B> + UsageProvider<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let mut finalized_heads = relay_chain.finalized_heads(para_id)?;

	loop {
		let finalized_head = if let Some(h) = finalized_heads.next().await {
			h
		} else {
			tracing::debug!(target: "cumulus-consensus", "Stopping following finalized head.");
			return Ok(());
		};

		let header = match Block::Header::decode(&mut &finalized_head[..]) {
			Ok(header) => header,
			Err(err) => {
				tracing::warn!(
					target: "cumulus-consensus",
					error = ?err,
					"Could not decode parachain header while following finalized heads.",
				);
				continue;
			}
		};

		let hash = header.hash();

		// don't finalize the same block multiple times.
		if parachain.usage_info().chain.finalized_hash != hash {
			if let Err(e) = parachain.finalize_block(BlockId::hash(hash), None, true) {
				match e {
					ClientError::UnknownBlock(_) => tracing::debug!(
						target: "cumulus-consensus",
						block_hash = ?hash,
						"Could not finalize block because it is unknown.",
					),
					_ => tracing::warn!(
						target: "cumulus-consensus",
						error = ?e,
						block_hash = ?hash,
						"Failed to finalize block",
					),
				}
			}
		}
	}
}

/// Run the parachain consensus.
///
/// This will follow the given `relay_chain` to act as consesus for the parachain that corresponds
/// to the given `para_id`. It will set the new best block of the parachain as it gets aware of it.
/// The same happens for the finalized block.
///
/// # Note
///
/// This will access the backend of the parachain and thus, this future should be spawned as blocking
/// task.
pub async fn run_parachain_consensus<P, R, Block, B>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
) -> ClientResult<()>
where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let follow_new_best = follow_new_best(
		para_id,
		parachain.clone(),
		relay_chain.clone(),
		announce_block,
	);
	let follow_finalized_head = follow_finalized_head(para_id, parachain, relay_chain);
	select! {
		r = follow_new_best.fuse() => r,
		r = follow_finalized_head.fuse() => r,
	}
}

/// Follow the relay chain new best head, to update the Parachain new best head.
async fn follow_new_best<P, R, Block, B>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
) -> ClientResult<()>
where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let mut new_best_heads = relay_chain.new_best_heads(para_id)?.fuse();
	let mut imported_blocks = parachain.import_notification_stream().fuse();
	// The unset best header of the parachain. Will be `Some(_)` when we have imported a relay chain
	// block before the parachain block it included. In this case we need to wait for this block to
	// be imported to set it as new best.
	let mut unset_best_header = None;

	loop {
		select! {
			h = new_best_heads.next() => {
				match h {
					Some(h) => handle_new_best_parachain_head(
						h,
						&*parachain,
						&*announce_block,
						&mut unset_best_header,
					),
					None => {
						tracing::debug!(
							target: "cumulus-consensus",
							"Stopping following new best.",
						);
						return Ok(())
					}
				}
			},
			i = imported_blocks.next() => {
				match i {
					Some(i) => handle_new_block_imported(
						i,
						&mut unset_best_header,
						&*parachain,
						&*announce_block,
					),
					None => {
						tracing::debug!(
							target: "cumulus-consensus",
							"Stopping following imported blocks.",
						);
						return Ok(())
					}
				}
			}
		}
	}
}

/// Handle a new import block of the parachain.
fn handle_new_block_imported<Block, P>(
	notification: BlockImportNotification<Block>,
	unset_best_header_opt: &mut Option<Block::Header>,
	parachain: &P,
	announce_block: &dyn Fn(Block::Hash, Vec<u8>),
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let unset_best_header = match (notification.is_new_best, &unset_best_header_opt) {
		// If this is the new best block or we don't have any unset block, we can end it here.
		(true, _) | (_, None) => return,
		(false, Some(ref u)) => u,
	};

	let unset_hash = if notification.header.number() < unset_best_header.number() {
		return;
	} else if notification.header.number() == unset_best_header.number() {
		let unset_hash = unset_best_header.hash();

		if unset_hash != notification.hash {
			return;
		} else {
			unset_hash
		}
	} else {
		unset_best_header.hash()
	};

	match parachain.block_status(&BlockId::Hash(unset_hash)) {
		Ok(BlockStatus::InChainWithState) => {
			drop(unset_best_header);
			let unset_best_header = unset_best_header_opt
				.take()
				.expect("We checked above that the value is set; qed");

			import_block_as_new_best(unset_hash, unset_best_header, parachain, announce_block);
		}
		state => tracing::debug!(
			target: "cumulus-consensus",
			unset_best_header = ?unset_best_header,
			imported_header = ?notification.header,
			?state,
			"Unexpected state for unset best header.",
		),
	}
}

/// Handle the new best parachain head as extracted from the new best relay chain.
fn handle_new_best_parachain_head<Block, P>(
	head: Vec<u8>,
	parachain: &P,
	announce_block: &dyn Fn(Block::Hash, Vec<u8>),
	unset_best_header: &mut Option<Block::Header>,
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let parachain_head = match <<Block as BlockT>::Header>::decode(&mut &head[..]) {
		Ok(header) => header,
		Err(err) => {
			tracing::warn!(
				target: "cumulus-consensus",
				error = ?err,
				"Could not decode Parachain header while following best heads.",
			);
			return;
		}
	};

	let hash = parachain_head.hash();

	if parachain.usage_info().chain.best_hash == hash {
		tracing::debug!(
			target: "cumulus-consensus",
			block_hash = ?hash,
			"Skipping set new best block, because block is already the best.",
		)
	} else {
		// Make sure the block is already known or otherwise we skip setting new best.
		match parachain.block_status(&BlockId::Hash(hash)) {
			Ok(BlockStatus::InChainWithState) => {
				unset_best_header.take();

				import_block_as_new_best(hash, parachain_head, parachain, announce_block);
			}
			Ok(BlockStatus::InChainPruned) => {
				tracing::error!(
					target: "cumulus-collator",
					block_hash = ?hash,
					"Trying to set pruned block as new best!",
				);
			}
			Ok(BlockStatus::Unknown) => {
				*unset_best_header = Some(parachain_head);

				tracing::debug!(
					target: "cumulus-collator",
					block_hash = ?hash,
					"Parachain block not yet imported, waiting for import to enact as best block.",
				);
			}
			Err(e) => {
				tracing::error!(
					target: "cumulus-collator",
					block_hash = ?hash,
					error = ?e,
					"Failed to get block status of block.",
				);
			}
			_ => {}
		}
	}
}

fn import_block_as_new_best<Block, P>(
	hash: Block::Hash,
	header: Block::Header,
	parachain: &P,
	announce_block: &dyn Fn(Block::Hash, Vec<u8>),
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	// Make it the new best block
	let mut block_import_params = BlockImportParams::new(BlockOrigin::ConsensusBroadcast, header);
	block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(true));
	block_import_params.import_existing = true;

	if let Err(err) = (&*parachain).import_block(block_import_params, Default::default()) {
		tracing::warn!(
			target: "cumulus-consensus",
			block_hash = ?hash,
			error = ?err,
			"Failed to set new best block.",
		);
	} else {
		(*announce_block)(hash, Vec::new());
	}
}

impl<T> RelaychainClient for Arc<T>
where
	T: sc_client_api::BlockchainEvents<PBlock> + ProvideRuntimeApi<PBlock> + 'static + Send + Sync,
	<T as ProvideRuntimeApi<PBlock>>::Api: ParachainHost<PBlock, Error = ClientError>,
{
	type Error = ClientError;

	type HeadStream = Box<dyn Stream<Item = Vec<u8>> + Send + Unpin>;

	fn new_best_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream> {
		let polkadot = self.clone();

		let s = self.import_notification_stream().filter_map(move |n| {
			future::ready(if n.is_new_best {
				polkadot
					.parachain_head_at(&BlockId::hash(n.hash), para_id)
					.ok()
					.and_then(|h| h)
			} else {
				None
			})
		});

		Ok(Box::new(s))
	}

	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream> {
		let polkadot = self.clone();

		let s = self.finality_notification_stream().filter_map(move |n| {
			future::ready(
				polkadot
					.parachain_head_at(&BlockId::hash(n.hash), para_id)
					.ok()
					.and_then(|h| h),
			)
		});

		Ok(Box::new(s))
	}

	fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> ClientResult<Option<Vec<u8>>> {
		self.runtime_api()
			.persisted_validation_data(at, para_id, OccupiedCoreAssumption::TimedOut)
			.map(|s| s.map(|s| s.parent_head.0))
	}
}

/// Select chain implementation for parachains.
///
/// The actual behavior of the implementation depends on the select chain implementation used by
/// Polkadot.
pub struct SelectChain<Block, PC, SC> {
	polkadot_client: PC,
	polkadot_select_chain: SC,
	para_id: ParaId,
	_marker: PhantomData<Block>,
}

impl<Block, PC, SC> SelectChain<Block, PC, SC> {
	/// Create new instance of `Self`.
	///
	/// - `para_id`: The id of the parachain.
	/// - `polkadot_client`: The client of the Polkadot node.
	/// - `polkadot_select_chain`: The Polkadot select chain implementation.
	pub fn new(para_id: ParaId, polkadot_client: PC, polkadot_select_chain: SC) -> Self {
		Self {
			polkadot_client,
			polkadot_select_chain,
			para_id,
			_marker: PhantomData,
		}
	}
}

impl<Block, PC: Clone, SC: Clone> Clone for SelectChain<Block, PC, SC> {
	fn clone(&self) -> Self {
		Self {
			polkadot_client: self.polkadot_client.clone(),
			polkadot_select_chain: self.polkadot_select_chain.clone(),
			para_id: self.para_id,
			_marker: PhantomData,
		}
	}
}

impl<Block, PC, SC> SelectChainT<Block> for SelectChain<Block, PC, SC>
where
	Block: BlockT,
	PC: RelaychainClient + Clone + Send + Sync,
	PC::Error: ToString,
	SC: SelectChainT<PBlock>,
{
	fn leaves(&self) -> Result<Vec<<Block as BlockT>::Hash>, ConsensusError> {
		let leaves = self.polkadot_select_chain.leaves()?;
		leaves
			.into_iter()
			.filter_map(|l| {
				self.polkadot_client
					.parachain_head_at(&BlockId::Hash(l), self.para_id)
					.map(|h| h.and_then(|d| <<Block as BlockT>::Hash>::decode(&mut &d[..]).ok()))
					.transpose()
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(|e| ConsensusError::ChainLookup(e.to_string()))
	}

	fn best_chain(&self) -> Result<<Block as BlockT>::Header, ConsensusError> {
		let best_chain = self.polkadot_select_chain.best_chain()?;
		let para_best_chain = self
			.polkadot_client
			.parachain_head_at(&BlockId::Hash(best_chain.hash()), self.para_id)
			.map_err(|e| ConsensusError::ChainLookup(e.to_string()))?;

		match para_best_chain {
			Some(best) => Decode::decode(&mut &best[..]).map_err(|e| {
				ConsensusError::ChainLookup(format!("Error decoding parachain head: {}", e))
			}),
			None => Err(ConsensusError::ChainLookup(
				"Could not find parachain head for best relay chain!".into(),
			)),
		}
	}
}

/// The result of [`ParachainConsensus::produce_candidate`].
pub struct ParachainCandidate<B> {
	/// The block that was build for this candidate.
	pub block: B,
	/// The proof that was recorded while building the block.
	pub proof: sp_trie::StorageProof,
}

/// A specific parachain consensus implementation that can be used by a collator to produce candidates.
///
/// The collator will call [`Self::produce_candidate`] every time there is a free core for the parachain
/// this collator is collating for. It is the job of the consensus implementation to decide if this
/// specific collator should build candidate for the given relay chain block. The consensus
/// implementation could for example check if this specific collator is part of the validator.
#[async_trait::async_trait]
pub trait ParachainConsensus<B: BlockT>: Send + Sync + dyn_clone::DynClone {
	/// Produce a new candidate at the given parent block.
	///
	/// Should return `None` if the consensus implementation decided that it shouldn't build a
	/// candidate or if there occurred any error.
	///
	/// # NOTE
	///
	/// It is expected that the block is already imported when the future resolves.
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>>;
}

dyn_clone::clone_trait_object!(<B> ParachainConsensus<B> where B: BlockT);

#[async_trait::async_trait]
impl<B: BlockT> ParachainConsensus<B> for Box<dyn ParachainConsensus<B> + Send + Sync> {
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>> {
		(*self).produce_candidate(parent, relay_parent, validation_data).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use cumulus_test_client::{
		runtime::{Block, Header},
		Client, InitBlockBuilder, TestClientBuilder, TestClientBuilderExt,
	};
	use futures::{channel::mpsc, executor::block_on};
	use futures_timer::Delay;
	use std::{sync::Mutex, time::Duration};

	struct RelaychainInner {
		new_best_heads: Option<mpsc::UnboundedReceiver<Header>>,
		finalized_heads: Option<mpsc::UnboundedReceiver<Header>>,
		new_best_heads_sender: mpsc::UnboundedSender<Header>,
		finalized_heads_sender: mpsc::UnboundedSender<Header>,
	}

	impl RelaychainInner {
		fn new() -> Self {
			let (new_best_heads_sender, new_best_heads) = mpsc::unbounded();
			let (finalized_heads_sender, finalized_heads) = mpsc::unbounded();

			Self {
				new_best_heads_sender,
				finalized_heads_sender,
				new_best_heads: Some(new_best_heads),
				finalized_heads: Some(finalized_heads),
			}
		}
	}

	#[derive(Clone)]
	struct Relaychain {
		inner: Arc<Mutex<RelaychainInner>>,
	}

	impl Relaychain {
		fn new() -> Self {
			Self {
				inner: Arc::new(Mutex::new(RelaychainInner::new())),
			}
		}
	}

	impl RelaychainClient for Relaychain {
		type Error = ClientError;

		type HeadStream = Box<dyn Stream<Item = Vec<u8>> + Send + Unpin>;
		fn new_best_heads(&self, _: ParaId) -> ClientResult<Self::HeadStream> {
			let stream = self
				.inner
				.lock()
				.unwrap()
				.new_best_heads
				.take()
				.expect("Should only be called once");

			Ok(Box::new(stream.map(|v| v.encode())))
		}

		fn finalized_heads(&self, _: ParaId) -> ClientResult<Self::HeadStream> {
			let stream = self
				.inner
				.lock()
				.unwrap()
				.finalized_heads
				.take()
				.expect("Should only be called once");

			Ok(Box::new(stream.map(|v| v.encode())))
		}

		fn parachain_head_at(
			&self,
			_: &BlockId<PBlock>,
			_: ParaId,
		) -> ClientResult<Option<Vec<u8>>> {
			unimplemented!("Not required for tests")
		}
	}

	fn build_and_import_block(mut client: Arc<Client>) -> Block {
		let builder = client.init_block_builder(None, Default::default());

		let block = builder.build().unwrap().block;
		let (header, body) = block.clone().deconstruct();

		let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, header);
		block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(false));
		block_import_params.body = Some(body);

		client
			.import_block(block_import_params, Default::default())
			.unwrap();
		assert_eq!(0, client.chain_info().best_number);

		block
	}

	#[test]
	fn follow_new_best_works() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::default().build());

		let block = build_and_import_block(client.clone());
		let relay_chain = Relaychain::new();
		let new_best_heads_sender = relay_chain
			.inner
			.lock()
			.unwrap()
			.new_best_heads_sender
			.clone();

		let consensus =
			run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}));

		let work = async move {
			new_best_heads_sender
				.unbounded_send(block.header().clone())
				.unwrap();
			loop {
				Delay::new(Duration::from_millis(100)).await;
				if block.hash() == client.usage_info().chain.best_hash {
					break;
				}
			}
		};

		block_on(async move {
			futures::pin_mut!(consensus);
			futures::pin_mut!(work);

			select! {
				r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
				_ = work.fuse() => {},
			}
		});
	}

	#[test]
	fn follow_finalized_works() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::default().build());

		let block = build_and_import_block(client.clone());
		let relay_chain = Relaychain::new();
		let finalized_sender = relay_chain
			.inner
			.lock()
			.unwrap()
			.finalized_heads_sender
			.clone();

		let consensus =
			run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}));

		let work = async move {
			finalized_sender
				.unbounded_send(block.header().clone())
				.unwrap();
			loop {
				Delay::new(Duration::from_millis(100)).await;
				if block.hash() == client.usage_info().chain.finalized_hash {
					break;
				}
			}
		};

		block_on(async move {
			futures::pin_mut!(consensus);
			futures::pin_mut!(work);

			select! {
				r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
				_ = work.fuse() => {},
			}
		});
	}

	#[test]
	fn follow_finalized_does_not_stop_on_unknown_block() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::default().build());

		let block = build_and_import_block(client.clone());

		let unknown_block = {
			let block_builder = client.init_block_builder_at(
				&BlockId::Hash(block.hash()),
				None,
				Default::default(),
			);
			block_builder.build().unwrap().block
		};

		let relay_chain = Relaychain::new();
		let finalized_sender = relay_chain
			.inner
			.lock()
			.unwrap()
			.finalized_heads_sender
			.clone();

		let consensus =
			run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}));

		let work = async move {
			for _ in 0..3usize {
				finalized_sender
					.unbounded_send(unknown_block.header().clone())
					.unwrap();

				Delay::new(Duration::from_millis(100)).await;
			}

			finalized_sender
				.unbounded_send(block.header().clone())
				.unwrap();
			loop {
				Delay::new(Duration::from_millis(100)).await;
				if block.hash() == client.usage_info().chain.finalized_hash {
					break;
				}
			}
		};

		block_on(async move {
			futures::pin_mut!(consensus);
			futures::pin_mut!(work);

			select! {
				r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
				_ = work.fuse() => {},
			}
		});
	}

	// It can happen that we first import a relay chain block, while not yet having the parachain
	// block imported that would be set to the best block. We need to make sure to import this
	// block as new best block in the moment it is imported.
	#[test]
	fn follow_new_best_sets_best_after_it_is_imported() {
		sp_tracing::try_init_simple();

		let mut client = Arc::new(TestClientBuilder::default().build());

		let block = build_and_import_block(client.clone());

		let unknown_block = {
			let block_builder = client.init_block_builder_at(
				&BlockId::Hash(block.hash()),
				None,
				Default::default(),
			);
			block_builder.build().unwrap().block
		};

		let relay_chain = Relaychain::new();
		let new_best_heads_sender = relay_chain
			.inner
			.lock()
			.unwrap()
			.new_best_heads_sender
			.clone();

		let consensus =
			run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}));

		let work = async move {
			new_best_heads_sender
				.unbounded_send(block.header().clone())
				.unwrap();

			loop {
				Delay::new(Duration::from_millis(100)).await;
				if block.hash() == client.usage_info().chain.best_hash {
					break;
				}
			}

			// Announce the unknown block
			new_best_heads_sender
				.unbounded_send(unknown_block.header().clone())
				.unwrap();

			// Do some iterations. As this is a local task executor, only one task can run at a time.
			// Meaning that it should already have processed the unknown block.
			for _ in 0..3usize {
				Delay::new(Duration::from_millis(100)).await;
			}

			let (header, body) = unknown_block.clone().deconstruct();

			let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, header);
			block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(false));
			block_import_params.body = Some(body);

			// Now import the unkown block to make it "known"
			client
				.import_block(block_import_params, Default::default())
				.unwrap();

			loop {
				Delay::new(Duration::from_millis(100)).await;
				if unknown_block.hash() == client.usage_info().chain.best_hash {
					break;
				}
			}
		};

		block_on(async move {
			futures::pin_mut!(consensus);
			futures::pin_mut!(work);

			select! {
				r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
				_ = work.fuse() => {},
			}
		});
	}
}
