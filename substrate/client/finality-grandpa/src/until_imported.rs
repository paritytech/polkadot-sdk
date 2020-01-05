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

//! Helper stream for waiting until one or more blocks are imported before
//! passing through inner items. This is done in a generic way to support
//! many different kinds of items.
//!
//! This is used for votes and commit messages currently.

use super::{
	BlockStatus as BlockStatusT,
	BlockSyncRequester as BlockSyncRequesterT,
	CommunicationIn,
	Error,
	SignedMessage,
};

use log::{debug, warn};
use sc_client_api::{BlockImportNotification, ImportNotifications};
use futures::prelude::*;
use futures::stream::Fuse;
use futures_timer::Delay;
use futures03::{StreamExt as _, TryStreamExt as _};
use finality_grandpa::voter;
use parking_lot::Mutex;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};

use std::collections::{HashMap, VecDeque};
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
use std::time::{Duration, Instant};
use sp_finality_grandpa::AuthorityId;

const LOG_PENDING_INTERVAL: Duration = Duration::from_secs(15);

// something which will block until imported.
pub(crate) trait BlockUntilImported<Block: BlockT>: Sized {
	// the type that is blocked on.
	type Blocked;

	/// new incoming item. For all internal items,
	/// check if they require to be waited for.
	/// if so, call the `Wait` closure.
	/// if they are ready, call the `Ready` closure.
	fn schedule_wait<S, Wait, Ready>(
		input: Self::Blocked,
		status_check: &S,
		wait: Wait,
		ready: Ready,
	) -> Result<(), Error> where
		S: BlockStatusT<Block>,
		Wait: FnMut(Block::Hash, NumberFor<Block>, Self),
		Ready: FnMut(Self::Blocked);

	/// called when the wait has completed. The canonical number is passed through
	/// for further checks.
	fn wait_completed(self, canon_number: NumberFor<Block>) -> Option<Self::Blocked>;
}

/// Buffering imported messages until blocks with given hashes are imported.
pub(crate) struct UntilImported<Block: BlockT, BlockStatus, BlockSyncRequester, I, M: BlockUntilImported<Block>> {
	import_notifications: Fuse<Box<dyn Stream<Item = BlockImportNotification<Block>, Error = ()> + Send>>,
	block_sync_requester: BlockSyncRequester,
	status_check: BlockStatus,
	inner: Fuse<I>,
	ready: VecDeque<M::Blocked>,
	check_pending: Box<dyn Stream<Item = (), Error = std::io::Error> + Send>,
	/// Mapping block hashes to their block number, the point in time it was
	/// first encountered (Instant) and a list of GRANDPA messages referencing
	/// the block hash.
	pending: HashMap<Block::Hash, (NumberFor<Block>, Instant, Vec<M>)>,
	identifier: &'static str,
}

impl<Block, BlockStatus, BlockSyncRequester, I, M> UntilImported<Block, BlockStatus, BlockSyncRequester, I, M> where
	Block: BlockT,
	BlockStatus: BlockStatusT<Block>,
	M: BlockUntilImported<Block>,
	I: Stream,
{
	/// Create a new `UntilImported` wrapper.
	pub(crate) fn new(
		import_notifications: ImportNotifications<Block>,
		block_sync_requester: BlockSyncRequester,
		status_check: BlockStatus,
		stream: I,
		identifier: &'static str,
	) -> Self {
		// how often to check if pending messages that are waiting for blocks to be
		// imported can be checked.
		//
		// the import notifications interval takes care of most of this; this is
		// used in the event of missed import notifications
		const CHECK_PENDING_INTERVAL: Duration = Duration::from_secs(5);

		let check_pending = futures03::stream::unfold(Delay::new(CHECK_PENDING_INTERVAL), |delay|
			Box::pin(async move {
				delay.await;
				Some(((), Delay::new(CHECK_PENDING_INTERVAL)))
			})).map(Ok).compat();

		UntilImported {
			import_notifications: {
				let stream = import_notifications.map::<_, fn(_) -> _>(|v| Ok::<_, ()>(v)).compat();
				Box::new(stream) as Box<dyn Stream<Item = _, Error = _> + Send>
			}.fuse(),
			block_sync_requester,
			status_check,
			inner: stream.fuse(),
			ready: VecDeque::new(),
			check_pending: Box::new(check_pending),
			pending: HashMap::new(),
			identifier,
		}
	}
}

impl<Block, BStatus, BSyncRequester, I, M> Stream for UntilImported<Block, BStatus, BSyncRequester, I, M> where
	Block: BlockT,
	BStatus: BlockStatusT<Block>,
	BSyncRequester: BlockSyncRequesterT<Block>,
	I: Stream<Item=M::Blocked,Error=Error>,
	M: BlockUntilImported<Block>,
{
	type Item = M::Blocked;
	type Error = Error;

	fn poll(&mut self) -> Poll<Option<M::Blocked>, Error> {
		loop {
			match self.inner.poll()? {
				Async::Ready(None) => return Ok(Async::Ready(None)),
				Async::Ready(Some(input)) => {
					// new input: schedule wait of any parts which require
					// blocks to be known.
					let ready = &mut self.ready;
					let pending = &mut self.pending;
					M::schedule_wait(
						input,
						&self.status_check,
						|target_hash, target_number, wait| pending
							.entry(target_hash)
							.or_insert_with(|| (target_number, Instant::now(), Vec::new()))
							.2
							.push(wait),
						|ready_item| ready.push_back(ready_item),
					)?;
				}
				Async::NotReady => break,
			}
		}

		loop {
			match self.import_notifications.poll() {
				Err(_) => return Err(Error::Network(format!("Failed to get new message"))),
				Ok(Async::Ready(None)) => return Ok(Async::Ready(None)),
				Ok(Async::Ready(Some(notification))) => {
					// new block imported. queue up all messages tied to that hash.
					if let Some((_, _, messages)) = self.pending.remove(&notification.hash) {
						let canon_number = notification.header.number().clone();
						let ready_messages = messages.into_iter()
							.filter_map(|m| m.wait_completed(canon_number));

						self.ready.extend(ready_messages);
				 	}
				}
				Ok(Async::NotReady) => break,
			}
		}

		let mut update_interval = false;
		while let Async::Ready(Some(_)) = self.check_pending.poll().map_err(Error::Timer)? {
			update_interval = true;
		}

		if update_interval {
			let mut known_keys = Vec::new();
			for (&block_hash, &mut (block_number, ref mut last_log, ref v)) in &mut self.pending {
				if let Some(number) = self.status_check.block_number(block_hash)? {
					known_keys.push((block_hash, number));
				} else {
					let next_log = *last_log + LOG_PENDING_INTERVAL;
					if Instant::now() >= next_log {
						debug!(
							target: "afg",
							"Waiting to import block {} before {} {} messages can be imported. \
							Requesting network sync service to retrieve block from. \
							Possible fork?",
							block_hash,
							v.len(),
							self.identifier,
						);

						// NOTE: when sending an empty vec of peers the
						// underlying should make a best effort to sync the
						// block from any peers it knows about.
						self.block_sync_requester.set_sync_fork_request(
							vec![],
							block_hash,
							block_number,
						);

						*last_log = next_log;
					}
				}
			}

			for (known_hash, canon_number) in known_keys {
				if let Some((_, _, pending_messages)) = self.pending.remove(&known_hash) {
					let ready_messages = pending_messages.into_iter()
						.filter_map(|m| m.wait_completed(canon_number));

					self.ready.extend(ready_messages);
				}
			}
		}

		if let Some(ready) = self.ready.pop_front() {
			return Ok(Async::Ready(Some(ready)))
		}

		if self.import_notifications.is_done() && self.inner.is_done() {
			Ok(Async::Ready(None))
		} else {
			Ok(Async::NotReady)
		}
	}
}

fn warn_authority_wrong_target<H: ::std::fmt::Display>(hash: H, id: AuthorityId) {
	warn!(
		target: "afg",
		"Authority {:?} signed GRANDPA message with \
		wrong block number for hash {}",
		id,
		hash,
	);
}

impl<Block: BlockT> BlockUntilImported<Block> for SignedMessage<Block> {
	type Blocked = Self;

	fn schedule_wait<BlockStatus, Wait, Ready>(
		msg: Self::Blocked,
		status_check: &BlockStatus,
		mut wait: Wait,
		mut ready: Ready,
	) -> Result<(), Error> where
		BlockStatus: BlockStatusT<Block>,
		Wait: FnMut(Block::Hash, NumberFor<Block>, Self),
		Ready: FnMut(Self::Blocked),
	{
		let (&target_hash, target_number) = msg.target();

		if let Some(number) = status_check.block_number(target_hash)? {
			if number != target_number {
				warn_authority_wrong_target(target_hash, msg.id);
			} else {
				ready(msg);
			}
		} else {
			wait(target_hash, target_number, msg)
		}

		Ok(())
	}

	fn wait_completed(self, canon_number: NumberFor<Block>) -> Option<Self::Blocked> {
		let (&target_hash, target_number) = self.target();
		if canon_number != target_number {
			warn_authority_wrong_target(target_hash, self.id);

			None
		} else {
			Some(self)
		}
	}
}

/// Helper type definition for the stream which waits until vote targets for
/// signed messages are imported.
pub(crate) type UntilVoteTargetImported<Block, BlockStatus, BlockSyncRequester, I> = UntilImported<
	Block,
	BlockStatus,
	BlockSyncRequester,
	I,
	SignedMessage<Block>,
>;

/// This blocks a global message import, i.e. a commit or catch up messages,
/// until all blocks referenced in its votes are known.
///
/// This is used for compact commits and catch up messages which have already
/// been checked for structural soundness (e.g. valid signatures).
pub(crate) struct BlockGlobalMessage<Block: BlockT> {
	inner: Arc<(AtomicUsize, Mutex<Option<CommunicationIn<Block>>>)>,
	target_number: NumberFor<Block>,
}

impl<Block: BlockT> BlockUntilImported<Block> for BlockGlobalMessage<Block> {
	type Blocked = CommunicationIn<Block>;

	fn schedule_wait<BlockStatus, Wait, Ready>(
		input: Self::Blocked,
		status_check: &BlockStatus,
		mut wait: Wait,
		mut ready: Ready,
	) -> Result<(), Error> where
		BlockStatus: BlockStatusT<Block>,
		Wait: FnMut(Block::Hash, NumberFor<Block>, Self),
		Ready: FnMut(Self::Blocked),
	{
		use std::collections::hash_map::Entry;

		enum KnownOrUnknown<N> {
			Known(N),
			Unknown(N),
		}

		impl<N> KnownOrUnknown<N> {
			fn number(&self) -> &N {
				match *self {
					KnownOrUnknown::Known(ref n) => n,
					KnownOrUnknown::Unknown(ref n) => n,
				}
			}
		}

		let mut checked_hashes: HashMap<_, KnownOrUnknown<NumberFor<Block>>> = HashMap::new();
		let mut unknown_count = 0;

		{
			// returns false when should early exit.
			let mut query_known = |target_hash, perceived_number| -> Result<bool, Error> {
				// check integrity: all votes for same hash have same number.
				let canon_number = match checked_hashes.entry(target_hash) {
					Entry::Occupied(entry) => entry.get().number().clone(),
					Entry::Vacant(entry) => {
						if let Some(number) = status_check.block_number(target_hash)? {
							entry.insert(KnownOrUnknown::Known(number));
							number

						} else {
							entry.insert(KnownOrUnknown::Unknown(perceived_number));
							unknown_count += 1;
							perceived_number
						}
					}
				};

				if canon_number != perceived_number {
					// invalid global message: messages targeting wrong number
					// or at least different from other vote in same global
					// message.
					return Ok(false);
				}

				Ok(true)
			};

			match input {
				voter::CommunicationIn::Commit(_, ref commit, ..) => {
					// add known hashes from all precommits.
					let precommit_targets = commit.precommits
						.iter()
						.map(|c| (c.target_number, c.target_hash));

					for (target_number, target_hash) in precommit_targets {
						if !query_known(target_hash, target_number)? {
							return Ok(())
						}
					}
				},
				voter::CommunicationIn::CatchUp(ref catch_up, ..) => {
					// add known hashes from all prevotes and precommits.
					let prevote_targets = catch_up.prevotes
						.iter()
						.map(|s| (s.prevote.target_number, s.prevote.target_hash));

					let precommit_targets = catch_up.precommits
						.iter()
						.map(|s| (s.precommit.target_number, s.precommit.target_hash));

					let targets = prevote_targets.chain(precommit_targets);

					for (target_number, target_hash) in targets {
						if !query_known(target_hash, target_number)? {
							return Ok(())
						}
					}
				},
			};
		}

		// none of the hashes in the global message were unknown.
		// we can just return the message directly.
		if unknown_count == 0 {
			ready(input);
			return Ok(())
		}

		let locked_global = Arc::new((AtomicUsize::new(unknown_count), Mutex::new(Some(input))));

		// schedule waits for all unknown messages.
		// when the last one of these has `wait_completed` called on it,
		// the global message will be returned.
		//
		// in the future, we may want to issue sync requests to the network
		// if this is taking a long time.
		for (hash, is_known) in checked_hashes {
			if let KnownOrUnknown::Unknown(target_number) = is_known {
				wait(hash, target_number, BlockGlobalMessage {
					inner: locked_global.clone(),
					target_number,
				})
			}
		}

		Ok(())
	}

	fn wait_completed(self, canon_number: NumberFor<Block>) -> Option<Self::Blocked> {
		if self.target_number != canon_number {
			// if we return without deducting the counter, then none of the other
			// handles can return the commit message.
			return None;
		}

		let mut last_count = self.inner.0.load(Ordering::Acquire);

		// CAS loop to ensure that we always have a last reader.
		loop {
			if last_count == 1 { // we are the last one left.
				return self.inner.1.lock().take();
			}

			let prev_value = self.inner.0.compare_and_swap(
				last_count,
				last_count - 1,
				Ordering::SeqCst,
			);

			if prev_value == last_count {
				return None;
			} else {
				last_count = prev_value;
			}
		}
	}
}

/// A stream which gates off incoming global messages, i.e. commit and catch up
/// messages, until all referenced block hashes have been imported.
pub(crate) type UntilGlobalMessageBlocksImported<Block, BlockStatus, BlockSyncRequester, I> = UntilImported<
	Block,
	BlockStatus,
	BlockSyncRequester,
	I,
	BlockGlobalMessage<Block>,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{CatchUp, CompactCommit};
	use tokio::runtime::current_thread::Runtime;
	use substrate_test_runtime_client::runtime::{Block, Hash, Header};
	use sp_consensus::BlockOrigin;
	use sc_client_api::BlockImportNotification;
	use futures::future::Either;
	use futures_timer::Delay;
	use futures03::{channel::mpsc, future::FutureExt as _, future::TryFutureExt as _};
	use finality_grandpa::Precommit;

	#[derive(Clone)]
	struct TestChainState {
		sender: mpsc::UnboundedSender<BlockImportNotification<Block>>,
		known_blocks: Arc<Mutex<HashMap<Hash, u64>>>,
	}

	impl TestChainState {
		fn new() -> (Self, ImportNotifications<Block>) {
			let (tx, rx) = mpsc::unbounded();
			let state = TestChainState {
				sender: tx,
				known_blocks: Arc::new(Mutex::new(HashMap::new())),
			};

			(state, rx)
		}

		fn block_status(&self) -> TestBlockStatus {
			TestBlockStatus { inner: self.known_blocks.clone() }
		}

		fn import_header(&self, header: Header) {
			let hash = header.hash();
			let number = header.number().clone();

			self.known_blocks.lock().insert(hash, number);
			self.sender.unbounded_send(BlockImportNotification {
				hash,
				origin: BlockOrigin::File,
				header,
				is_new_best: false,
				retracted: vec![],
			}).unwrap();
		}
	}

	struct TestBlockStatus {
		inner: Arc<Mutex<HashMap<Hash, u64>>>,
	}

	impl BlockStatusT<Block> for TestBlockStatus {
		fn block_number(&self, hash: Hash) -> Result<Option<u64>, Error> {
			Ok(self.inner.lock().get(&hash).map(|x| x.clone()))
		}
	}

	#[derive(Clone)]
	struct TestBlockSyncRequester {
		requests: Arc<Mutex<Vec<(Hash, NumberFor<Block>)>>>,
	}

	impl Default for TestBlockSyncRequester {
		fn default() -> Self {
			TestBlockSyncRequester {
				requests: Arc::new(Mutex::new(Vec::new())),
			}
		}
	}

	impl BlockSyncRequesterT<Block> for TestBlockSyncRequester {
		fn set_sync_fork_request(&self, _peers: Vec<sc_network::PeerId>, hash: Hash, number: NumberFor<Block>) {
			self.requests.lock().push((hash, number));
		}
	}

	fn make_header(number: u64) -> Header {
		Header::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		)
	}

	// unwrap the commit from `CommunicationIn` returning its fields in a tuple,
	// panics if the given message isn't a commit
	fn unapply_commit(msg: CommunicationIn<Block>) -> (u64, CompactCommit::<Block>) {
		match msg {
			voter::CommunicationIn::Commit(round, commit, ..) => (round, commit),
			_ => panic!("expected commit"),
		}
	}

	// unwrap the catch up from `CommunicationIn` returning its inner representation,
	// panics if the given message isn't a catch up
	fn unapply_catch_up(msg: CommunicationIn<Block>) -> CatchUp<Block> {
		match msg {
			voter::CommunicationIn::CatchUp(catch_up, ..) => catch_up,
			_ => panic!("expected catch up"),
		}
	}

	fn message_all_dependencies_satisfied<F>(
		msg: CommunicationIn<Block>,
		enact_dependencies: F,
	) -> CommunicationIn<Block> where
		F: FnOnce(&TestChainState),
	{
		let (chain_state, import_notifications) = TestChainState::new();
		let block_status = chain_state.block_status();

		// enact all dependencies before importing the message
		enact_dependencies(&chain_state);

		let (global_tx, global_rx) = futures::sync::mpsc::unbounded();

		let until_imported = UntilGlobalMessageBlocksImported::new(
			import_notifications,
			TestBlockSyncRequester::default(),
			block_status,
			global_rx.map_err(|_| panic!("should never error")),
			"global",
		);

		global_tx.unbounded_send(msg).unwrap();

		let work = until_imported.into_future();

		let mut runtime = Runtime::new().unwrap();
		runtime.block_on(work).map_err(|(e, _)| e).unwrap().0.unwrap()
	}

	fn blocking_message_on_dependencies<F>(
		msg: CommunicationIn<Block>,
		enact_dependencies: F,
	) -> CommunicationIn<Block> where
		F: FnOnce(&TestChainState),
	{
		let (chain_state, import_notifications) = TestChainState::new();
		let block_status = chain_state.block_status();

		let (global_tx, global_rx) = futures::sync::mpsc::unbounded();

		let until_imported = UntilGlobalMessageBlocksImported::new(
			import_notifications,
			TestBlockSyncRequester::default(),
			block_status,
			global_rx.map_err(|_| panic!("should never error")),
			"global",
		);

		global_tx.unbounded_send(msg).unwrap();

		// NOTE: needs to be cloned otherwise it is moved to the stream and
		// dropped too early.
		let inner_chain_state = chain_state.clone();
		let work = until_imported
			.into_future()
			.select2(Delay::new(Duration::from_millis(100)).unit_error().compat())
			.then(move |res| match res {
				Err(_) => panic!("neither should have had error"),
				Ok(Either::A(_)) => panic!("timeout should have fired first"),
				Ok(Either::B((_, until_imported))) => {
					// timeout fired. push in the headers.
					enact_dependencies(&inner_chain_state);

					until_imported
				}
			});

		let mut runtime = Runtime::new().unwrap();
		runtime.block_on(work).map_err(|(e, _)| e).unwrap().0.unwrap()
	}

	#[test]
	fn blocking_commit_message() {
		let h1 = make_header(5);
		let h2 = make_header(6);
		let h3 = make_header(7);

		let unknown_commit = CompactCommit::<Block> {
			target_hash: h1.hash(),
			target_number: 5,
			precommits: vec![
				Precommit {
					target_hash: h2.hash(),
					target_number: 6,
				},
				Precommit {
					target_hash: h3.hash(),
					target_number: 7,
				},
			],
			auth_data: Vec::new(), // not used
		};

		let unknown_commit = || voter::CommunicationIn::Commit(
			0,
			unknown_commit.clone(),
			voter::Callback::Blank,
		);

		let res = blocking_message_on_dependencies(
			unknown_commit(),
			|chain_state| {
				chain_state.import_header(h1);
				chain_state.import_header(h2);
				chain_state.import_header(h3);
			},
		);

		assert_eq!(
			unapply_commit(res),
			unapply_commit(unknown_commit()),
		);
	}

	#[test]
	fn commit_message_all_known() {
		let h1 = make_header(5);
		let h2 = make_header(6);
		let h3 = make_header(7);

		let known_commit = CompactCommit::<Block> {
			target_hash: h1.hash(),
			target_number: 5,
			precommits: vec![
				Precommit {
					target_hash: h2.hash(),
					target_number: 6,
				},
				Precommit {
					target_hash: h3.hash(),
					target_number: 7,
				},
			],
			auth_data: Vec::new(), // not used
		};

		let known_commit = || voter::CommunicationIn::Commit(
			0,
			known_commit.clone(),
			voter::Callback::Blank,
		);

		let res = message_all_dependencies_satisfied(
			known_commit(),
			|chain_state| {
				chain_state.import_header(h1);
				chain_state.import_header(h2);
				chain_state.import_header(h3);
			},
		);

		assert_eq!(
			unapply_commit(res),
			unapply_commit(known_commit()),
		);
	}

	#[test]
	fn blocking_catch_up_message() {
		let h1 = make_header(5);
		let h2 = make_header(6);
		let h3 = make_header(7);

		let signed_prevote = |header: &Header| {
			finality_grandpa::SignedPrevote {
				id: Default::default(),
				signature: Default::default(),
				prevote: finality_grandpa::Prevote {
					target_hash: header.hash(),
					target_number: *header.number(),
				},
			}
		};

		let signed_precommit = |header: &Header| {
			finality_grandpa::SignedPrecommit {
				id: Default::default(),
				signature: Default::default(),
				precommit: finality_grandpa::Precommit {
					target_hash: header.hash(),
					target_number: *header.number(),
				},
			}
		};

		let prevotes = vec![
			signed_prevote(&h1),
			signed_prevote(&h3),
		];

		let precommits = vec![
			signed_precommit(&h1),
			signed_precommit(&h2),
		];

		let unknown_catch_up = finality_grandpa::CatchUp {
			round_number: 1,
			prevotes,
			precommits,
			base_hash: h1.hash(),
			base_number: *h1.number(),
		};

		let unknown_catch_up = || voter::CommunicationIn::CatchUp(
			unknown_catch_up.clone(),
			voter::Callback::Blank,
		);

		let res = blocking_message_on_dependencies(
			unknown_catch_up(),
			|chain_state| {
				chain_state.import_header(h1);
				chain_state.import_header(h2);
				chain_state.import_header(h3);
			},
		);

		assert_eq!(
			unapply_catch_up(res),
			unapply_catch_up(unknown_catch_up()),
		);
	}

	#[test]
	fn catch_up_message_all_known() {
		let h1 = make_header(5);
		let h2 = make_header(6);
		let h3 = make_header(7);

		let signed_prevote = |header: &Header| {
			finality_grandpa::SignedPrevote {
				id: Default::default(),
				signature: Default::default(),
				prevote: finality_grandpa::Prevote {
					target_hash: header.hash(),
					target_number: *header.number(),
				},
			}
		};

		let signed_precommit = |header: &Header| {
			finality_grandpa::SignedPrecommit {
				id: Default::default(),
				signature: Default::default(),
				precommit: finality_grandpa::Precommit {
					target_hash: header.hash(),
					target_number: *header.number(),
				},
			}
		};

		let prevotes = vec![
			signed_prevote(&h1),
			signed_prevote(&h3),
		];

		let precommits = vec![
			signed_precommit(&h1),
			signed_precommit(&h2),
		];

		let unknown_catch_up = finality_grandpa::CatchUp {
			round_number: 1,
			prevotes,
			precommits,
			base_hash: h1.hash(),
			base_number: *h1.number(),
		};

		let unknown_catch_up = || voter::CommunicationIn::CatchUp(
			unknown_catch_up.clone(),
			voter::Callback::Blank,
		);

		let res = message_all_dependencies_satisfied(
			unknown_catch_up(),
			|chain_state| {
				chain_state.import_header(h1);
				chain_state.import_header(h2);
				chain_state.import_header(h3);
			},
		);

		assert_eq!(
			unapply_catch_up(res),
			unapply_catch_up(unknown_catch_up()),
		);
	}

	#[test]
	fn request_block_sync_for_needed_blocks() {
		let (chain_state, import_notifications) = TestChainState::new();
		let block_status = chain_state.block_status();

		let (global_tx, global_rx) = futures::sync::mpsc::unbounded();

		let block_sync_requester = TestBlockSyncRequester::default();

		let until_imported = UntilGlobalMessageBlocksImported::new(
			import_notifications,
			block_sync_requester.clone(),
			block_status,
			global_rx.map_err(|_| panic!("should never error")),
			"global",
		);

		let h1 = make_header(5);
		let h2 = make_header(6);
		let h3 = make_header(7);

		// we create a commit message, with precommits for blocks 6 and 7 which
		// we haven't imported.
		let unknown_commit = CompactCommit::<Block> {
			target_hash: h1.hash(),
			target_number: 5,
			precommits: vec![
				Precommit {
					target_hash: h2.hash(),
					target_number: 6,
				},
				Precommit {
					target_hash: h3.hash(),
					target_number: 7,
				},
			],
			auth_data: Vec::new(), // not used
		};

		let unknown_commit = || voter::CommunicationIn::Commit(
			0,
			unknown_commit.clone(),
			voter::Callback::Blank,
		);

		// we send the commit message and spawn the until_imported stream
		global_tx.unbounded_send(unknown_commit()).unwrap();

		let mut runtime = Runtime::new().unwrap();
		runtime.spawn(until_imported.into_future().map(|_| ()).map_err(|_| ()));

		// assert that we will make sync requests
		let assert = futures::future::poll_fn::<(), (), _>(|| {
			let block_sync_requests = block_sync_requester.requests.lock();

			// we request blocks targeted by the precommits that aren't imported
			if block_sync_requests.contains(&(h2.hash(), *h2.number())) &&
				block_sync_requests.contains(&(h3.hash(), *h3.number()))
			{
				return Ok(Async::Ready(()));
			}

			Ok(Async::NotReady)
		});

		// the `until_imported` stream doesn't request the blocks immediately,
		// but it should request them after a small timeout
		let timeout = Delay::new(Duration::from_secs(60)).unit_error().compat();
		let test = assert.select2(timeout).map(|res| match res {
			Either::A(_) => {},
			Either::B(_) => panic!("timed out waiting for block sync request"),
		}).map_err(|_| ());

		runtime.block_on(test).unwrap();
	}
}
