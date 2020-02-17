// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

//! Proof of work consensus for Substrate.
//!
//! To use this engine, you can need to have a struct that implements
//! `PowAlgorithm`. After that, pass an instance of the struct, along
//! with other necessary client references to `import_queue` to setup
//! the queue. Use the `start_mine` function for basic CPU mining.
//!
//! The auxiliary storage for PoW engine only stores the total difficulty.
//! For other storage requirements for particular PoW algorithm (such as
//! the actual difficulty for each particular blocks), you can take a client
//! reference in your `PowAlgorithm` implementation, and use a separate prefix
//! for the auxiliary storage. It is also possible to just use the runtime
//! as the storage, but it is not recommended as it won't work well with light
//! clients.

use std::sync::Arc;
use std::any::Any;
use std::borrow::Cow;
use std::thread;
use std::collections::HashMap;
use std::marker::PhantomData;
use sc_client_api::{BlockOf, backend::AuxStore};
use sp_blockchain::{HeaderBackend, ProvideCache, well_known_cache_keys::Id as CacheKeyId};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_runtime::{Justification, RuntimeString};
use sp_runtime::generic::{BlockId, Digest, DigestItem};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_api::ProvideRuntimeApi;
use sp_consensus_pow::{Seal, TotalDifficulty, POW_ENGINE_ID};
use sp_inherents::{InherentDataProviders, InherentData};
use sp_consensus::{
	BlockImportParams, BlockOrigin, ForkChoiceStrategy, SyncOracle, Environment, Proposer,
	SelectChain, Error as ConsensusError, CanAuthorWith, RecordProof, BlockImport,
	BlockCheckParams, ImportResult,
};
use sp_consensus::import_queue::{BoxBlockImport, BasicQueue, Verifier};
use codec::{Encode, Decode};
use sc_client_api;
use log::*;
use sp_timestamp::{InherentError as TIError, TimestampInherentData};

#[derive(derive_more::Display, Debug)]
pub enum Error<B: BlockT> {
	#[display(fmt = "Header uses the wrong engine {:?}", _0)]
	WrongEngine([u8; 4]),
	#[display(fmt = "Header {:?} is unsealed", _0)]
	HeaderUnsealed(B::Hash),
	#[display(fmt = "PoW validation error: invalid seal")]
	InvalidSeal,
	#[display(fmt = "PoW validation error: preliminary verification failed")]
	FailedPreliminaryVerify,
	#[display(fmt = "Rejecting block too far in future")]
	TooFarInFuture,
	#[display(fmt = "Fetching best header failed using select chain: {:?}", _0)]
	BestHeaderSelectChain(ConsensusError),
	#[display(fmt = "Fetching best header failed: {:?}", _0)]
	BestHeader(sp_blockchain::Error),
	#[display(fmt = "Best header does not exist")]
	NoBestHeader,
	#[display(fmt = "Block proposing error: {:?}", _0)]
	BlockProposingError(String),
	#[display(fmt = "Fetch best hash failed via select chain: {:?}", _0)]
	BestHashSelectChain(ConsensusError),
	#[display(fmt = "Error with block built on {:?}: {:?}", _0, _1)]
	BlockBuiltError(B::Hash, ConsensusError),
	#[display(fmt = "Creating inherents failed: {}", _0)]
	CreateInherents(sp_inherents::Error),
	#[display(fmt = "Checking inherents failed: {}", _0)]
	CheckInherents(String),
	Client(sp_blockchain::Error),
	Codec(codec::Error),
	Environment(String),
	Runtime(RuntimeString)
}

impl<B: BlockT> std::convert::From<Error<B>> for String {
	fn from(error: Error<B>) -> String {
		error.to_string()
	}
}

impl<B: BlockT> std::convert::From<Error<B>> for ConsensusError {
	fn from(error: Error<B>) -> ConsensusError {
		ConsensusError::ClientImport(error.to_string())
	}
}

/// Auxiliary storage prefix for PoW engine.
pub const POW_AUX_PREFIX: [u8; 4] = *b"PoW:";

/// Get the auxiliary storage key used by engine to store total difficulty.
fn aux_key<T: AsRef<[u8]>>(hash: &T) -> Vec<u8> {
	POW_AUX_PREFIX.iter().chain(hash.as_ref()).copied().collect()
}

/// Intermediate value passed to block importer.
#[derive(Encode, Decode, Clone, Debug, Default)]
pub struct PowIntermediate<Difficulty> {
	/// Difficulty of the block, if known.
	pub difficulty: Option<Difficulty>,
}

/// Intermediate key for PoW engine.
pub static INTERMEDIATE_KEY: &[u8] = b"pow1";

/// Auxiliary storage data for PoW.
#[derive(Encode, Decode, Clone, Debug, Default)]
pub struct PowAux<Difficulty> {
	/// Difficulty of the current block.
	pub difficulty: Difficulty,
	/// Total difficulty up to current block.
	pub total_difficulty: Difficulty,
}

impl<Difficulty> PowAux<Difficulty> where
	Difficulty: Decode + Default,
{
	/// Read the auxiliary from client.
	pub fn read<C: AuxStore, B: BlockT>(client: &C, hash: &B::Hash) -> Result<Self, Error<B>> {
		let key = aux_key(&hash);

		match client.get_aux(&key).map_err(Error::Client)? {
			Some(bytes) => Self::decode(&mut &bytes[..]).map_err(Error::Codec),
			None => Ok(Self::default()),
		}
	}
}

/// Algorithm used for proof of work.
pub trait PowAlgorithm<B: BlockT> {
	/// Difficulty for the algorithm.
	type Difficulty: TotalDifficulty + Default + Encode + Decode + Ord + Clone + Copy;

	/// Get the next block's difficulty.
	///
	/// This function will be called twice during the import process, so the implementation
	/// should be properly cached.
	fn difficulty(&self, parent: &BlockId<B>) -> Result<Self::Difficulty, Error<B>>;
	/// Verify that the seal is valid against given pre hash when parent block is not yet imported.
	///
	/// None means that preliminary verify is not available for this algorithm.
	fn preliminary_verify(
		&self,
		_pre_hash: &B::Hash,
		_seal: &Seal,
	) -> Result<Option<bool>, Error<B>> {
		Ok(None)
	}
	/// Verify that the difficulty is valid against given seal.
	fn verify(
		&self,
		parent: &BlockId<B>,
		pre_hash: &B::Hash,
		seal: &Seal,
		difficulty: Self::Difficulty,
	) -> Result<bool, Error<B>>;
	/// Mine a seal that satisfies the given difficulty.
	fn mine(
		&self,
		parent: &BlockId<B>,
		pre_hash: &B::Hash,
		difficulty: Self::Difficulty,
		round: u32,
	) -> Result<Option<Seal>, Error<B>>;
}

/// A block importer for PoW.
pub struct PowBlockImport<B: BlockT, I, C, S, Algorithm> {
	algorithm: Algorithm,
	inner: I,
	select_chain: Option<S>,
	client: Arc<C>,
	inherent_data_providers: sp_inherents::InherentDataProviders,
	check_inherents_after: <<B as BlockT>::Header as HeaderT>::Number,
}

impl<B: BlockT, I: Clone, C, S: Clone, Algorithm: Clone> Clone for PowBlockImport<B, I, C, S, Algorithm> {
	fn clone(&self) -> Self {
		Self {
			algorithm: self.algorithm.clone(),
			inner: self.inner.clone(),
			select_chain: self.select_chain.clone(),
			client: self.client.clone(),
			inherent_data_providers: self.inherent_data_providers.clone(),
			check_inherents_after: self.check_inherents_after.clone(),
		}
	}
}

impl<B, I, C, S, Algorithm> PowBlockImport<B, I, C, S, Algorithm> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
	Algorithm: PowAlgorithm<B>,
{
	/// Create a new block import suitable to be used in PoW
	pub fn new(
		inner: I,
		client: Arc<C>,
		algorithm: Algorithm,
		check_inherents_after: <<B as BlockT>::Header as HeaderT>::Number,
		select_chain: Option<S>,
		inherent_data_providers: sp_inherents::InherentDataProviders,
	) -> Self {
		Self { inner, client, algorithm, check_inherents_after,
			   select_chain, inherent_data_providers }
	}

	fn check_inherents(
		&self,
		block: B,
		block_id: BlockId<B>,
		inherent_data: InherentData,
		timestamp_now: u64,
	) -> Result<(), Error<B>> {
		const MAX_TIMESTAMP_DRIFT_SECS: u64 = 60;

		if *block.header().number() < self.check_inherents_after {
			return Ok(())
		}

		let inherent_res = self.client.runtime_api().check_inherents(
			&block_id,
			block,
			inherent_data,
		).map_err(Error::Client)?;

		if !inherent_res.ok() {
			inherent_res
				.into_errors()
				.try_for_each(|(i, e)| match TIError::try_from(&i, &e) {
					Some(TIError::ValidAtTimestamp(timestamp)) => {
						if timestamp > timestamp_now + MAX_TIMESTAMP_DRIFT_SECS {
							return Err(Error::TooFarInFuture);
						}

						Ok(())
					},
					Some(TIError::Other(e)) => Err(Error::Runtime(e)),
					None => Err(Error::CheckInherents(
						self.inherent_data_providers.error_to_string(&i, &e)
					)),
				})
		} else {
			Ok(())
		}
	}
}

impl<B, I, C, S, Algorithm> BlockImport<B> for PowBlockImport<B, I, C, S, Algorithm> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	S: SelectChain<B>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
	Algorithm: PowAlgorithm<B>,
	Algorithm::Difficulty: 'static,
{
	type Error = ConsensusError;
	type Transaction = sp_api::TransactionFor<C, B>;

	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).map_err(Into::into)
	}

	fn import_block(
		&mut self,
		mut block: BlockImportParams<B, Self::Transaction>,
		new_cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		let best_hash = match self.select_chain.as_ref() {
			Some(select_chain) => select_chain.best_chain()
				.map_err(|e| format!("Fetch best chain failed via select chain: {:?}", e))?
				.hash(),
			None => self.client.info().best_hash,
		};

		let parent_hash = *block.header.parent_hash();
		let best_aux = PowAux::read::<_, B>(self.client.as_ref(), &best_hash)?;
		let mut aux = PowAux::read::<_, B>(self.client.as_ref(), &parent_hash)?;

		if let Some(inner_body) = block.body.take() {
			let inherent_data = self.inherent_data_providers
				.create_inherent_data().map_err(|e| e.into_string())?;
			let timestamp_now = inherent_data.timestamp_inherent_data().map_err(|e| e.into_string())?;

			let check_block = B::new(block.header.clone(), inner_body);

			self.check_inherents(
				check_block.clone(),
				BlockId::Hash(parent_hash),
				inherent_data,
				timestamp_now
			)?;

			block.body = Some(check_block.deconstruct().1);
		}

		let inner_seal = match block.post_digests.last() {
			Some(DigestItem::Seal(id, seal)) => {
				if id == &POW_ENGINE_ID {
					seal.clone()
				} else {
					return Err(Error::<B>::WrongEngine(*id).into())
				}
			},
			_ => return Err(Error::<B>::HeaderUnsealed(block.header.hash()).into()),
		};

		let intermediate = block.take_intermediate::<PowIntermediate::<Algorithm::Difficulty>>(
			INTERMEDIATE_KEY
		)?;

		let difficulty = match intermediate.difficulty {
			Some(difficulty) => difficulty,
			None => self.algorithm.difficulty(&BlockId::hash(parent_hash))?,
		};

		let pre_hash = block.header.hash();
		if !self.algorithm.verify(
			&BlockId::hash(parent_hash),
			&pre_hash,
			&inner_seal,
			difficulty,
		)? {
			return Err(Error::<B>::InvalidSeal.into())
		}

		aux.difficulty = difficulty;
		aux.total_difficulty.increment(difficulty);

		let key = aux_key(&block.post_hash());
		block.auxiliary.push((key, Some(aux.encode())));
		if block.fork_choice.is_none() {
			block.fork_choice = Some(ForkChoiceStrategy::Custom(
				aux.total_difficulty > best_aux.total_difficulty
			));
		}

		self.inner.import_block(block, new_cache).map_err(Into::into)
	}
}

/// A verifier for PoW blocks.
pub struct PowVerifier<B: BlockT, Algorithm> {
	algorithm: Algorithm,
	_marker: PhantomData<B>,
}

impl<B: BlockT, Algorithm> PowVerifier<B, Algorithm> {
	pub fn new(
		algorithm: Algorithm,
	) -> Self {
		Self { algorithm, _marker: PhantomData }
	}

	fn check_header(
		&self,
		mut header: B::Header,
	) -> Result<(B::Header, DigestItem<B::Hash>), Error<B>> where
		Algorithm: PowAlgorithm<B>,
	{
		let hash = header.hash();

		let (seal, inner_seal) = match header.digest_mut().pop() {
			Some(DigestItem::Seal(id, seal)) => {
				if id == POW_ENGINE_ID {
					(DigestItem::Seal(id, seal.clone()), seal)
				} else {
					return Err(Error::WrongEngine(id))
				}
			},
			_ => return Err(Error::HeaderUnsealed(hash)),
		};

		let pre_hash = header.hash();

		if !self.algorithm.preliminary_verify(&pre_hash, &inner_seal)?.unwrap_or(true) {
			return Err(Error::FailedPreliminaryVerify);
		}

		Ok((header, seal))
	}
}

impl<B: BlockT, Algorithm> Verifier<B> for PowVerifier<B, Algorithm> where
	Algorithm: PowAlgorithm<B> + Send + Sync,
	Algorithm::Difficulty: 'static,
{
	fn verify(
		&mut self,
		origin: BlockOrigin,
		header: B::Header,
		justification: Option<Justification>,
		body: Option<Vec<B::Extrinsic>>,
	) -> Result<(BlockImportParams<B, ()>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
		let hash = header.hash();
		let (checked_header, seal) = self.check_header(header)?;

		let intermediate = PowIntermediate::<Algorithm::Difficulty> {
			difficulty: None,
		};

		let mut import_block = BlockImportParams::new(origin, checked_header);
		import_block.post_digests.push(seal);
		import_block.body = body;
		import_block.justification = justification;
		import_block.intermediates.insert(
			Cow::from(INTERMEDIATE_KEY),
			Box::new(intermediate) as Box<dyn Any>
		);
		import_block.post_hash = Some(hash);

		Ok((import_block, None))
	}
}

/// Register the PoW inherent data provider, if not registered already.
pub fn register_pow_inherent_data_provider(
	inherent_data_providers: &InherentDataProviders,
) -> Result<(), sp_consensus::Error> {
	if !inherent_data_providers.has_provider(&sp_timestamp::INHERENT_IDENTIFIER) {
		inherent_data_providers
			.register_provider(sp_timestamp::InherentDataProvider)
			.map_err(Into::into)
			.map_err(sp_consensus::Error::InherentData)
	} else {
		Ok(())
	}
}

/// The PoW import queue type.
pub type PowImportQueue<B, Transaction> = BasicQueue<B, Transaction>;

/// Import queue for PoW engine.
pub fn import_queue<B, Transaction, Algorithm>(
	block_import: BoxBlockImport<B, Transaction>,
	algorithm: Algorithm,
	inherent_data_providers: InherentDataProviders,
) -> Result<
	PowImportQueue<B, Transaction>,
	sp_consensus::Error
> where
	B: BlockT,
	Transaction: Send + Sync + 'static,
	Algorithm: PowAlgorithm<B> + Clone + Send + Sync + 'static,
{
	register_pow_inherent_data_provider(&inherent_data_providers)?;

	let verifier = PowVerifier::new(algorithm);

	Ok(BasicQueue::new(
		verifier,
		block_import,
		None,
		None
	))
}

/// Start the background mining thread for PoW. Note that because PoW mining
/// is CPU-intensive, it is not possible to use an async future to define this.
/// However, it's not recommended to use background threads in the rest of the
/// codebase.
///
/// `preruntime` is a parameter that allows a custom additional pre-runtime
/// digest to be inserted for blocks being built. This can encode authorship
/// information, or just be a graffiti. `round` is for number of rounds the
/// CPU miner runs each time. This parameter should be tweaked so that each
/// mining round is within sub-second time.
pub fn start_mine<B: BlockT, C, Algorithm, E, SO, S, CAW>(
	mut block_import: BoxBlockImport<B, sp_api::TransactionFor<C, B>>,
	client: Arc<C>,
	algorithm: Algorithm,
	mut env: E,
	preruntime: Option<Vec<u8>>,
	round: u32,
	mut sync_oracle: SO,
	build_time: std::time::Duration,
	select_chain: Option<S>,
	inherent_data_providers: sp_inherents::InherentDataProviders,
	can_author_with: CAW,
) where
	C: HeaderBackend<B> + AuxStore + ProvideRuntimeApi<B> + 'static,
	Algorithm: PowAlgorithm<B> + Send + Sync + 'static,
	E: Environment<B> + Send + Sync + 'static,
	E::Error: std::fmt::Debug,
	E::Proposer: Proposer<B, Transaction = sp_api::TransactionFor<C, B>>,
	SO: SyncOracle + Send + Sync + 'static,
	S: SelectChain<B> + 'static,
	CAW: CanAuthorWith<B> + Send + 'static,
{
	if let Err(_) = register_pow_inherent_data_provider(&inherent_data_providers) {
		warn!("Registering inherent data provider for timestamp failed");
	}

	thread::spawn(move || {
		loop {
			match mine_loop(
				&mut block_import,
				client.as_ref(),
				&algorithm,
				&mut env,
				preruntime.as_ref(),
				round,
				&mut sync_oracle,
				build_time.clone(),
				select_chain.as_ref(),
				&inherent_data_providers,
				&can_author_with,
			) {
				Ok(()) => (),
				Err(e) => error!(
					"Mining block failed with {:?}. Sleep for 1 second before restarting...",
					e
				),
			}
			std::thread::sleep(std::time::Duration::new(1, 0));
		}
	});
}

fn mine_loop<B: BlockT, C, Algorithm, E, SO, S, CAW>(
	block_import: &mut BoxBlockImport<B, sp_api::TransactionFor<C, B>>,
	client: &C,
	algorithm: &Algorithm,
	env: &mut E,
	preruntime: Option<&Vec<u8>>,
	round: u32,
	sync_oracle: &mut SO,
	build_time: std::time::Duration,
	select_chain: Option<&S>,
	inherent_data_providers: &sp_inherents::InherentDataProviders,
	can_author_with: &CAW,
) -> Result<(), Error<B>> where
	C: HeaderBackend<B> + AuxStore + ProvideRuntimeApi<B>,
	Algorithm: PowAlgorithm<B>,
	Algorithm::Difficulty: 'static,
	E: Environment<B>,
	E::Proposer: Proposer<B, Transaction = sp_api::TransactionFor<C, B>>,
	E::Error: std::fmt::Debug,
	SO: SyncOracle,
	S: SelectChain<B>,
	sp_api::TransactionFor<C, B>: 'static,
	CAW: CanAuthorWith<B>,
{
	'outer: loop {
		if sync_oracle.is_major_syncing() {
			debug!(target: "pow", "Skipping proposal due to sync.");
			std::thread::sleep(std::time::Duration::new(1, 0));
			continue 'outer
		}

		let (best_hash, best_header) = match select_chain {
			Some(select_chain) => {
				let header = select_chain.best_chain()
					.map_err(Error::BestHeaderSelectChain)?;
				let hash = header.hash();
				(hash, header)
			},
			None => {
				let hash = client.info().best_hash;
				let header = client.header(BlockId::Hash(hash))
					.map_err(Error::BestHeader)?
					.ok_or(Error::NoBestHeader)?;
				(hash, header)
			},
		};

		if let Err(err) = can_author_with.can_author_with(&BlockId::Hash(best_hash)) {
			warn!(
				target: "pow",
				"Skipping proposal `can_author_with` returned: {} \
				Probably a node update is required!",
				err,
			);
			std::thread::sleep(std::time::Duration::from_secs(1));
			continue 'outer
		}

		let mut proposer = futures::executor::block_on(env.init(&best_header))
			.map_err(|e| Error::Environment(format!("{:?}", e)))?;

		let inherent_data = inherent_data_providers
			.create_inherent_data().map_err(Error::CreateInherents)?;
		let mut inherent_digest = Digest::default();
		if let Some(preruntime) = &preruntime {
			inherent_digest.push(DigestItem::PreRuntime(POW_ENGINE_ID, preruntime.to_vec()));
		}
		let proposal = futures::executor::block_on(proposer.propose(
			inherent_data,
			inherent_digest,
			build_time.clone(),
			RecordProof::No,
		)).map_err(|e| Error::BlockProposingError(format!("{:?}", e)))?;

		let (header, body) = proposal.block.deconstruct();
		let (difficulty, seal) = {
			let difficulty = algorithm.difficulty(
				&BlockId::Hash(best_hash),
			)?;

			loop {
				let seal = algorithm.mine(
					&BlockId::Hash(best_hash),
					&header.hash(),
					difficulty,
					round,
				)?;

				if let Some(seal) = seal {
					break (difficulty, seal)
				}

				if best_hash != client.info().best_hash {
					continue 'outer
				}
			}
		};

		let (hash, seal) = {
			let seal = DigestItem::Seal(POW_ENGINE_ID, seal);
			let mut header = header.clone();
			header.digest_mut().push(seal);
			let hash = header.hash();
			let seal = header.digest_mut().pop()
				.expect("Pushed one seal above; length greater than zero; qed");
			(hash, seal)
		};

		let intermediate = PowIntermediate::<Algorithm::Difficulty> {
			difficulty: Some(difficulty),
		};

		let mut import_block = BlockImportParams::new(BlockOrigin::Own, header);
		import_block.post_digests.push(seal);
		import_block.body = Some(body);
		import_block.storage_changes = Some(proposal.storage_changes);
		import_block.intermediates.insert(
			Cow::from(INTERMEDIATE_KEY),
			Box::new(intermediate) as Box<dyn Any>
		);
		import_block.post_hash = Some(hash);

		block_import.import_block(import_block, HashMap::default())
			.map_err(|e| Error::BlockBuiltError(best_hash, e))?;
	}
}
