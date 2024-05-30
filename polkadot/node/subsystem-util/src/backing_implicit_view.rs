// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use futures::channel::oneshot;
use polkadot_node_subsystem::{
	errors::ChainApiError,
	messages::{ChainApiMessage, ProspectiveParachainsMessage, RuntimeApiMessage},
	SubsystemSender,
};
use polkadot_primitives::{BlockNumber, Hash, Id as ParaId};

use std::collections::HashMap;

use crate::{
	request_session_index_for_child,
	runtime::{self, prospective_parachains_mode, recv_runtime, ProspectiveParachainsMode},
};

// Always aim to retain 1 block before the active leaves.
const MINIMUM_RETAIN_LENGTH: BlockNumber = 2;

/// Handles the implicit view of the relay chain derived from the immediate view, which
/// is composed of active leaves, and the minimum relay-parents allowed for
/// candidates of various parachains at those leaves.
#[derive(Clone)]
pub struct View {
	leaves: HashMap<Hash, ActiveLeafPruningInfo>,
	block_info_storage: HashMap<Hash, BlockInfo>,
	collating_for: Option<ParaId>,
}

impl View {
	/// Create a new empty view.
	/// If `collating_for` is `Some`, the node is a collator and is only interested in the allowed
	/// relay parents of a single paraid. When this is true, prospective-parachains is no longer
	/// queried.
	pub fn new(collating_for: Option<ParaId>) -> Self {
		Self { leaves: Default::default(), block_info_storage: Default::default(), collating_for }
	}
}

impl Default for View {
	fn default() -> Self {
		Self::new(None)
	}
}

// Minimum relay parents implicitly relative to a particular block.
#[derive(Debug, Clone)]
struct AllowedRelayParents {
	// minimum relay parents can only be fetched for active leaves,
	// so this will be empty for all blocks that haven't ever been
	// witnessed as active leaves.
	minimum_relay_parents: HashMap<ParaId, BlockNumber>,
	// Ancestry, in descending order, starting from the block hash itself down
	// to and including the minimum of `minimum_relay_parents`.
	allowed_relay_parents_contiguous: Vec<Hash>,
}

impl AllowedRelayParents {
	fn allowed_relay_parents_for(
		&self,
		para_id: Option<ParaId>,
		base_number: BlockNumber,
	) -> &[Hash] {
		let para_id = match para_id {
			None => return &self.allowed_relay_parents_contiguous[..],
			Some(p) => p,
		};

		let para_min = match self.minimum_relay_parents.get(&para_id) {
			Some(p) => *p,
			None => return &[],
		};

		if base_number < para_min {
			return &[]
		}

		let diff = base_number - para_min;

		// difference of 0 should lead to slice len of 1
		let slice_len = ((diff + 1) as usize).min(self.allowed_relay_parents_contiguous.len());
		&self.allowed_relay_parents_contiguous[..slice_len]
	}
}

#[derive(Debug, Clone)]
struct ActiveLeafPruningInfo {
	// The minimum block in the same branch of the relay-chain that should be
	// preserved.
	retain_minimum: BlockNumber,
}

#[derive(Debug, Clone)]
struct BlockInfo {
	block_number: BlockNumber,
	// If this was previously an active leaf, this will be `Some`
	// and is useful for understanding the views of peers in the network
	// which may not be in perfect synchrony with our own view.
	//
	// If they are ahead of us in getting a new leaf, there's nothing we
	// can do as it's an unrecognized block hash. But if they're behind us,
	// it's useful for us to retain some information about previous leaves'
	// implicit views so we can continue to send relevant messages to them
	// until they catch up.
	maybe_allowed_relay_parents: Option<AllowedRelayParents>,
	parent_hash: Hash,
}

impl View {
	/// Get an iterator over active leaves in the view.
	pub fn leaves(&self) -> impl Iterator<Item = &Hash> {
		self.leaves.keys()
	}

	/// Activate a leaf in the view.
	/// This will request the minimum relay parents the leaf and will load headers in the
	/// ancestry of the leaf as needed. These are the 'implicit ancestors' of the leaf.
	///
	/// To maximize reuse of outdated leaves, it's best to activate new leaves before
	/// deactivating old ones.
	///
	/// The allowed relay parents for the relevant paras under this leaf can be
	/// queried with [`View::known_allowed_relay_parents_under`].
	///
	/// No-op for known leaves.
	pub async fn activate_leaf<Sender>(
		&mut self,
		sender: &mut Sender,
		leaf_hash: Hash,
	) -> Result<(), FetchError>
	where
		Sender: SubsystemSender<ChainApiMessage>
			+ SubsystemSender<ProspectiveParachainsMessage>
			+ SubsystemSender<RuntimeApiMessage>,
	{
		if self.leaves.contains_key(&leaf_hash) {
			return Err(FetchError::AlreadyKnown)
		}

		let res = fetch_fresh_leaf_and_insert_ancestry(
			leaf_hash,
			&mut self.block_info_storage,
			&mut *sender,
			self.collating_for,
		)
		.await;

		match res {
			Ok(fetched) => {
				// Retain at least `MINIMUM_RETAIN_LENGTH` blocks in storage.
				// This helps to avoid Chain API calls when activating leaves in the
				// same chain.
				let retain_minimum = std::cmp::min(
					fetched.minimum_ancestor_number,
					fetched.leaf_number.saturating_sub(MINIMUM_RETAIN_LENGTH),
				);

				self.leaves.insert(leaf_hash, ActiveLeafPruningInfo { retain_minimum });

				Ok(())
			},
			Err(e) => Err(e),
		}
	}

	/// Deactivate a leaf in the view. This prunes any outdated implicit ancestors as well.
	///
	/// Returns hashes of blocks pruned from storage.
	pub fn deactivate_leaf(&mut self, leaf_hash: Hash) -> Vec<Hash> {
		let mut removed = Vec::new();

		if self.leaves.remove(&leaf_hash).is_none() {
			return removed
		}

		// Prune everything before the minimum out of all leaves,
		// pruning absolutely everything if there are no leaves (empty view)
		//
		// Pruning by block number does leave behind orphaned forks slightly longer
		// but the memory overhead is negligible.
		{
			let minimum = self.leaves.values().map(|l| l.retain_minimum).min();

			self.block_info_storage.retain(|hash, i| {
				let keep = minimum.map_or(false, |m| i.block_number >= m);
				if !keep {
					removed.push(*hash);
				}
				keep
			});

			removed
		}
	}

	/// Get an iterator over all allowed relay-parents in the view with no particular order.
	///
	/// **Important**: not all blocks are guaranteed to be allowed for some leaves, it may
	/// happen that a block info is only kept in the view storage because of a retaining rule.
	///
	/// For getting relay-parents that are valid for parachain candidates use
	/// [`View::known_allowed_relay_parents_under`].
	pub fn all_allowed_relay_parents(&self) -> impl Iterator<Item = &Hash> {
		self.block_info_storage.keys()
	}

	/// Get the known, allowed relay-parents that are valid for parachain candidates
	/// which could be backed in a child of a given block for a given para ID.
	///
	/// This is expressed as a contiguous slice of relay-chain block hashes which may
	/// include the provided block hash itself.
	///
	/// If `para_id` is `None`, this returns all valid relay-parents across all paras
	/// for the leaf.
	///
	/// `None` indicates that the block hash isn't part of the implicit view or that
	/// there are no known allowed relay parents.
	///
	/// This always returns `Some` for active leaves or for blocks that previously
	/// were active leaves.
	///
	/// This can return the empty slice, which indicates that no relay-parents are allowed
	/// for the para, e.g. if the para is not scheduled at the given block hash.
	pub fn known_allowed_relay_parents_under(
		&self,
		block_hash: &Hash,
		para_id: Option<ParaId>,
	) -> Option<&[Hash]> {
		let block_info = self.block_info_storage.get(block_hash)?;
		block_info
			.maybe_allowed_relay_parents
			.as_ref()
			.map(|mins| mins.allowed_relay_parents_for(para_id, block_info.block_number))
	}
}

/// Errors when fetching a leaf and associated ancestry.
#[fatality::fatality]
pub enum FetchError {
	/// Activated leaf is already present in view.
	#[error("Leaf was already known")]
	AlreadyKnown,

	/// Request to the prospective parachains subsystem failed.
	#[error("The prospective parachains subsystem was unavailable")]
	ProspectiveParachainsUnavailable,

	/// Failed to fetch the block header.
	#[error("A block header was unavailable")]
	BlockHeaderUnavailable(Hash, BlockHeaderUnavailableReason),

	/// A block header was unavailable due to a chain API error.
	#[error("A block header was unavailable due to a chain API error")]
	ChainApiError(Hash, ChainApiError),

	/// Request to the Chain API subsystem failed.
	#[error("The chain API subsystem was unavailable")]
	ChainApiUnavailable,

	/// Request to the runtime API failed.
	#[error("Runtime API error: {0}")]
	RuntimeApi(#[from] runtime::Error),
}

/// Reasons a block header might have been unavailable.
#[derive(Debug)]
pub enum BlockHeaderUnavailableReason {
	/// Block header simply unknown.
	Unknown,
	/// Internal Chain API error.
	Internal(ChainApiError),
	/// The subsystem was unavailable.
	SubsystemUnavailable,
}

struct FetchSummary {
	minimum_ancestor_number: BlockNumber,
	leaf_number: BlockNumber,
}

// Request the min relay parents from prospective-parachains.
async fn fetch_min_relay_parents_from_prospective_parachains<
	Sender: SubsystemSender<ProspectiveParachainsMessage>,
>(
	leaf_hash: Hash,
	sender: &mut Sender,
) -> Result<Vec<(ParaId, BlockNumber)>, FetchError> {
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(ProspectiveParachainsMessage::GetMinimumRelayParents(leaf_hash, tx))
		.await;

	rx.await.map_err(|_| FetchError::ProspectiveParachainsUnavailable)
}

// Request the min relay parent for the purposes of a collator, directly using ChainApi (where
// prospective-parachains is not available).
async fn fetch_min_relay_parents_for_collator<Sender>(
	leaf_hash: Hash,
	leaf_number: BlockNumber,
	sender: &mut Sender,
) -> Result<Option<BlockNumber>, FetchError>
where
	Sender: SubsystemSender<ProspectiveParachainsMessage>
		+ SubsystemSender<RuntimeApiMessage>
		+ SubsystemSender<ChainApiMessage>,
{
	let Ok(ProspectiveParachainsMode::Enabled { allowed_ancestry_len, .. }) =
		prospective_parachains_mode(sender, leaf_hash).await
	else {
		// This should never happen, leaves that don't have prospective parachains mode enabled
		// should not use implicit view.
		return Ok(None)
	};

	// Fetch the session of the leaf. We must make sure that we stop at the ancestor which has a
	// different session index.
	let required_session =
		recv_runtime(request_session_index_for_child(leaf_hash, sender).await).await?;

	let mut min = leaf_number;

	// Fetch the ancestors, up to allowed_ancestry_len.
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(ChainApiMessage::Ancestors {
			hash: leaf_hash,
			k: allowed_ancestry_len,
			response_channel: tx,
		})
		.await;
	let hashes = rx
		.await
		.map_err(|_| FetchError::ChainApiUnavailable)?
		.map_err(|err| FetchError::ChainApiError(leaf_hash, err))?;

	for hash in hashes {
		// The relay chain cannot accept blocks backed from previous sessions, with
		// potentially previous validators. This is a technical limitation we need to
		// respect here.
		let session = recv_runtime(request_session_index_for_child(hash, sender).await).await?;

		if session == required_session {
			// We should never underflow here, the ChainAPI stops at genesis block.
			min = min.saturating_sub(1);
		} else {
			break
		}
	}

	Ok(Some(min))
}

async fn fetch_fresh_leaf_and_insert_ancestry<Sender>(
	leaf_hash: Hash,
	block_info_storage: &mut HashMap<Hash, BlockInfo>,
	sender: &mut Sender,
	collating_for: Option<ParaId>,
) -> Result<FetchSummary, FetchError>
where
	Sender: SubsystemSender<ChainApiMessage>
		+ SubsystemSender<ProspectiveParachainsMessage>
		+ SubsystemSender<RuntimeApiMessage>,
{
	let leaf_header = {
		let (tx, rx) = oneshot::channel();
		sender.send_message(ChainApiMessage::BlockHeader(leaf_hash, tx)).await;

		match rx.await {
			Ok(Ok(Some(header))) => header,
			Ok(Ok(None)) =>
				return Err(FetchError::BlockHeaderUnavailable(
					leaf_hash,
					BlockHeaderUnavailableReason::Unknown,
				)),
			Ok(Err(e)) =>
				return Err(FetchError::BlockHeaderUnavailable(
					leaf_hash,
					BlockHeaderUnavailableReason::Internal(e),
				)),
			Err(_) =>
				return Err(FetchError::BlockHeaderUnavailable(
					leaf_hash,
					BlockHeaderUnavailableReason::SubsystemUnavailable,
				)),
		}
	};

	// If the node is a collator, bypass prospective-parachains. We're only interested in the one
	// paraid and the subsystem is not present.
	let min_relay_parents = if let Some(para_id) = collating_for {
		fetch_min_relay_parents_for_collator(leaf_hash, leaf_header.number, sender)
			.await?
			.map(|x| vec![(para_id, x)])
			.unwrap_or_default()
	} else {
		fetch_min_relay_parents_from_prospective_parachains(leaf_hash, sender).await?
	};

	let min_min = min_relay_parents.iter().map(|x| x.1).min().unwrap_or(leaf_header.number);
	let expected_ancestry_len = (leaf_header.number.saturating_sub(min_min) as usize) + 1;

	let ancestry = if leaf_header.number > 0 {
		let mut next_ancestor_number = leaf_header.number - 1;
		let mut next_ancestor_hash = leaf_header.parent_hash;

		let mut ancestry = Vec::with_capacity(expected_ancestry_len);
		ancestry.push(leaf_hash);

		// Ensure all ancestors up to and including `min_min` are in the
		// block storage. When views advance incrementally, everything
		// should already be present.
		while next_ancestor_number >= min_min {
			let parent_hash = if let Some(info) = block_info_storage.get(&next_ancestor_hash) {
				info.parent_hash
			} else {
				// load the header and insert into block storage.
				let (tx, rx) = oneshot::channel();
				sender.send_message(ChainApiMessage::BlockHeader(next_ancestor_hash, tx)).await;

				let header = match rx.await {
					Ok(Ok(Some(header))) => header,
					Ok(Ok(None)) =>
						return Err(FetchError::BlockHeaderUnavailable(
							next_ancestor_hash,
							BlockHeaderUnavailableReason::Unknown,
						)),
					Ok(Err(e)) =>
						return Err(FetchError::BlockHeaderUnavailable(
							next_ancestor_hash,
							BlockHeaderUnavailableReason::Internal(e),
						)),
					Err(_) =>
						return Err(FetchError::BlockHeaderUnavailable(
							next_ancestor_hash,
							BlockHeaderUnavailableReason::SubsystemUnavailable,
						)),
				};

				block_info_storage.insert(
					next_ancestor_hash,
					BlockInfo {
						block_number: next_ancestor_number,
						parent_hash: header.parent_hash,
						maybe_allowed_relay_parents: None,
					},
				);

				header.parent_hash
			};

			ancestry.push(next_ancestor_hash);
			if next_ancestor_number == 0 {
				break
			}

			next_ancestor_number -= 1;
			next_ancestor_hash = parent_hash;
		}

		ancestry
	} else {
		vec![leaf_hash]
	};

	let fetched_ancestry =
		FetchSummary { minimum_ancestor_number: min_min, leaf_number: leaf_header.number };

	let allowed_relay_parents = AllowedRelayParents {
		minimum_relay_parents: min_relay_parents.into_iter().collect(),
		allowed_relay_parents_contiguous: ancestry,
	};

	let leaf_block_info = BlockInfo {
		parent_hash: leaf_header.parent_hash,
		block_number: leaf_header.number,
		maybe_allowed_relay_parents: Some(allowed_relay_parents),
	};

	block_info_storage.insert(leaf_hash, leaf_block_info);

	Ok(fetched_ancestry)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::TimeoutExt;
	use assert_matches::assert_matches;
	use futures::future::{join, FutureExt};
	use polkadot_node_subsystem::{messages::RuntimeApiRequest, AllMessages};
	use polkadot_node_subsystem_test_helpers::{
		make_subsystem_context, TestSubsystemContextHandle,
	};
	use polkadot_overseer::SubsystemContext;
	use polkadot_primitives::{AsyncBackingParams, Header};
	use sp_core::testing::TaskExecutor;
	use std::time::Duration;

	const PARA_A: ParaId = ParaId::new(0);
	const PARA_B: ParaId = ParaId::new(1);
	const PARA_C: ParaId = ParaId::new(2);

	const GENESIS_HASH: Hash = Hash::repeat_byte(0xFF);
	const GENESIS_NUMBER: BlockNumber = 0;

	// Chains A and B are forks of genesis.

	const CHAIN_A: &[Hash] =
		&[Hash::repeat_byte(0x01), Hash::repeat_byte(0x02), Hash::repeat_byte(0x03)];

	const CHAIN_B: &[Hash] = &[
		Hash::repeat_byte(0x04),
		Hash::repeat_byte(0x05),
		Hash::repeat_byte(0x06),
		Hash::repeat_byte(0x07),
		Hash::repeat_byte(0x08),
		Hash::repeat_byte(0x09),
	];

	type VirtualOverseer = TestSubsystemContextHandle<AllMessages>;

	const TIMEOUT: Duration = Duration::from_secs(2);

	async fn overseer_recv(virtual_overseer: &mut VirtualOverseer) -> AllMessages {
		virtual_overseer
			.recv()
			.timeout(TIMEOUT)
			.await
			.expect("overseer `recv` timed out")
	}

	fn default_header() -> Header {
		Header {
			parent_hash: Hash::zero(),
			number: 0,
			state_root: Hash::zero(),
			extrinsics_root: Hash::zero(),
			digest: Default::default(),
		}
	}

	fn get_block_header(chain: &[Hash], hash: &Hash) -> Option<Header> {
		let idx = chain.iter().position(|h| h == hash)?;
		let parent_hash = idx.checked_sub(1).map(|i| chain[i]).unwrap_or(GENESIS_HASH);
		let number =
			if *hash == GENESIS_HASH { GENESIS_NUMBER } else { GENESIS_NUMBER + idx as u32 + 1 };
		Some(Header { parent_hash, number, ..default_header() })
	}

	async fn assert_block_header_requests(
		virtual_overseer: &mut VirtualOverseer,
		chain: &[Hash],
		blocks: &[Hash],
	) {
		for block in blocks.iter().rev() {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::ChainApi(
					ChainApiMessage::BlockHeader(hash, tx)
				) => {
					assert_eq!(*block, hash, "unexpected block header request");
					let header = if block == &GENESIS_HASH {
						Header {
							number: GENESIS_NUMBER,
							..default_header()
						}
					} else {
						get_block_header(chain, block).expect("unknown block")
					};

					tx.send(Ok(Some(header))).unwrap();
				}
			);
		}
	}

	async fn assert_min_relay_parents_request(
		virtual_overseer: &mut VirtualOverseer,
		leaf: &Hash,
		response: Vec<(ParaId, u32)>,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetMinimumRelayParents(
					leaf_hash,
					tx
				)
			) => {
				assert_eq!(*leaf, leaf_hash, "received unexpected leaf hash");
				tx.send(response).unwrap();
			}
		);
	}

	async fn assert_async_backing_params_request(
		virtual_overseer: &mut VirtualOverseer,
		leaf: Hash,
		params: AsyncBackingParams,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(
					leaf_hash,
					RuntimeApiRequest::AsyncBackingParams(
						tx
					)
				)
			) => {
				assert_eq!(leaf, leaf_hash, "received unexpected leaf hash");
				tx.send(Ok(params)).unwrap();
			}
		);
	}

	async fn assert_session_index_request(
		virtual_overseer: &mut VirtualOverseer,
		leaf: Hash,
		session: u32,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(
					leaf_hash,
					RuntimeApiRequest::SessionIndexForChild(
						tx
					)
				)
			) => {
				assert_eq!(leaf, leaf_hash, "received unexpected leaf hash");
				tx.send(Ok(session)).unwrap();
			}
		);
	}

	async fn assert_ancestors_request(
		virtual_overseer: &mut VirtualOverseer,
		leaf: Hash,
		expected_ancestor_len: u32,
		response: Vec<Hash>,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::ChainApi(
				ChainApiMessage::Ancestors {
					hash: leaf_hash,
					k,
					response_channel: tx
				}
			) => {
				assert_eq!(leaf, leaf_hash, "received unexpected leaf hash");
				assert_eq!(k, expected_ancestor_len as usize);

				tx.send(Ok(response)).unwrap();
			}
		);
	}

	#[test]
	fn construct_fresh_view() {
		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

		let mut view = View::default();

		assert_eq!(view.collating_for, None);

		// Chain B.
		const PARA_A_MIN_PARENT: u32 = 4;
		const PARA_B_MIN_PARENT: u32 = 3;

		let prospective_response = vec![(PARA_A, PARA_A_MIN_PARENT), (PARA_B, PARA_B_MIN_PARENT)];

		let leaf = CHAIN_B.last().unwrap();
		let leaf_idx = CHAIN_B.len() - 1;
		let min_min_idx = (PARA_B_MIN_PARENT - GENESIS_NUMBER - 1) as usize;

		let fut = view.activate_leaf(ctx.sender(), *leaf).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &CHAIN_B[leaf_idx..]).await;
			assert_min_relay_parents_request(&mut ctx_handle, leaf, prospective_response).await;
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &CHAIN_B[min_min_idx..leaf_idx])
				.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		for i in min_min_idx..(CHAIN_B.len() - 1) {
			// No allowed relay parents constructed for ancestry.
			assert!(view.known_allowed_relay_parents_under(&CHAIN_B[i], None).is_none());
		}

		let leaf_info =
			view.block_info_storage.get(leaf).expect("block must be present in storage");
		assert_matches!(
			leaf_info.maybe_allowed_relay_parents,
			Some(ref allowed_relay_parents) => {
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_A], PARA_A_MIN_PARENT);
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_B], PARA_B_MIN_PARENT);
				let expected_ancestry: Vec<Hash> =
					CHAIN_B[min_min_idx..].iter().rev().copied().collect();
				assert_eq!(
					allowed_relay_parents.allowed_relay_parents_contiguous,
					expected_ancestry
				);

				assert_eq!(view.known_allowed_relay_parents_under(&leaf, None), Some(&expected_ancestry[..]));
				assert_eq!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_A)), Some(&expected_ancestry[..(PARA_A_MIN_PARENT - 1) as usize]));
				assert_eq!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_B)), Some(&expected_ancestry[..]));
				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_C)).unwrap().is_empty());
			}
		);

		// Suppose the whole test chain A is allowed up to genesis for para C.
		const PARA_C_MIN_PARENT: u32 = 0;
		let prospective_response = vec![(PARA_C, PARA_C_MIN_PARENT)];
		let leaf = CHAIN_A.last().unwrap();
		let blocks = [&[GENESIS_HASH], CHAIN_A].concat();
		let leaf_idx = blocks.len() - 1;

		let fut = view.activate_leaf(ctx.sender(), *leaf).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, CHAIN_A, &blocks[leaf_idx..]).await;
			assert_min_relay_parents_request(&mut ctx_handle, leaf, prospective_response).await;
			assert_block_header_requests(&mut ctx_handle, CHAIN_A, &blocks[..leaf_idx]).await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		assert_eq!(view.leaves.len(), 2);

		let leaf_info =
			view.block_info_storage.get(leaf).expect("block must be present in storage");
		assert_matches!(
			leaf_info.maybe_allowed_relay_parents,
			Some(ref allowed_relay_parents) => {
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_C], GENESIS_NUMBER);
				let expected_ancestry: Vec<Hash> =
					blocks[..].iter().rev().copied().collect();
				assert_eq!(
					allowed_relay_parents.allowed_relay_parents_contiguous,
					expected_ancestry
				);

				assert_eq!(view.known_allowed_relay_parents_under(&leaf, None), Some(&expected_ancestry[..]));
				assert_eq!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_C)), Some(&expected_ancestry[..]));

				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_A)).unwrap().is_empty());
				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_B)).unwrap().is_empty());
			}
		);
	}

	#[test]
	fn construct_fresh_view_single_para() {
		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

		let mut view = View::new(Some(PARA_A));

		assert_eq!(view.collating_for, Some(PARA_A));

		// Chain B.
		const PARA_A_MIN_PARENT: u32 = 4;

		let current_session = 2;

		let leaf = CHAIN_B.last().unwrap();
		let leaf_idx = CHAIN_B.len() - 1;
		let min_min_idx = (PARA_A_MIN_PARENT - GENESIS_NUMBER - 1) as usize;

		let fut = view.activate_leaf(ctx.sender(), *leaf).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &CHAIN_B[leaf_idx..]).await;

			assert_async_backing_params_request(
				&mut ctx_handle,
				*leaf,
				AsyncBackingParams {
					max_candidate_depth: 0,
					allowed_ancestry_len: PARA_A_MIN_PARENT,
				},
			)
			.await;

			assert_session_index_request(&mut ctx_handle, *leaf, current_session).await;

			assert_ancestors_request(
				&mut ctx_handle,
				*leaf,
				PARA_A_MIN_PARENT,
				CHAIN_B[min_min_idx..leaf_idx].iter().copied().rev().collect(),
			)
			.await;

			for hash in CHAIN_B[min_min_idx..leaf_idx].into_iter().rev() {
				assert_session_index_request(&mut ctx_handle, *hash, current_session).await;
			}

			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &CHAIN_B[min_min_idx..leaf_idx])
				.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		for i in min_min_idx..(CHAIN_B.len() - 1) {
			// No allowed relay parents constructed for ancestry.
			assert!(view.known_allowed_relay_parents_under(&CHAIN_B[i], None).is_none());
		}

		let leaf_info =
			view.block_info_storage.get(leaf).expect("block must be present in storage");
		assert_matches!(
			leaf_info.maybe_allowed_relay_parents,
			Some(ref allowed_relay_parents) => {
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_A], PARA_A_MIN_PARENT);
				let expected_ancestry: Vec<Hash> =
					CHAIN_B[min_min_idx..].iter().rev().copied().collect();
				assert_eq!(
					allowed_relay_parents.allowed_relay_parents_contiguous,
					expected_ancestry
				);

				assert_eq!(view.known_allowed_relay_parents_under(&leaf, None), Some(&expected_ancestry[..]));
				assert_eq!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_A)), Some(&expected_ancestry[..]));

				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_B)).unwrap().is_empty());
				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_C)).unwrap().is_empty());
			}
		);

		// Suppose the whole test chain A is allowed up to genesis for para A, but the genesis block
		// is in a different session.
		let leaf = CHAIN_A.last().unwrap();
		let blocks = [&[GENESIS_HASH], CHAIN_A].concat();
		let leaf_idx = blocks.len() - 1;

		let fut = view.activate_leaf(ctx.sender(), *leaf).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});

		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, CHAIN_A, &blocks[leaf_idx..]).await;

			assert_async_backing_params_request(
				&mut ctx_handle,
				*leaf,
				AsyncBackingParams {
					max_candidate_depth: 0,
					allowed_ancestry_len: blocks.len() as u32,
				},
			)
			.await;

			assert_session_index_request(&mut ctx_handle, *leaf, current_session).await;

			assert_ancestors_request(
				&mut ctx_handle,
				*leaf,
				blocks.len() as u32,
				blocks[..leaf_idx].iter().rev().copied().collect(),
			)
			.await;

			for hash in blocks[1..leaf_idx].into_iter().rev() {
				assert_session_index_request(&mut ctx_handle, *hash, current_session).await;
			}

			assert_session_index_request(&mut ctx_handle, GENESIS_HASH, 0).await;

			// We won't request for the genesis block
			assert_block_header_requests(&mut ctx_handle, CHAIN_A, &blocks[1..leaf_idx]).await;
		};

		futures::executor::block_on(join(fut, overseer_fut));

		assert_eq!(view.leaves.len(), 2);

		let leaf_info =
			view.block_info_storage.get(leaf).expect("block must be present in storage");
		assert_matches!(
			leaf_info.maybe_allowed_relay_parents,
			Some(ref allowed_relay_parents) => {
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_A], 1);
				let expected_ancestry: Vec<Hash> =
					CHAIN_A[..].iter().rev().copied().collect();
				assert_eq!(
					allowed_relay_parents.allowed_relay_parents_contiguous,
					expected_ancestry
				);

				assert_eq!(view.known_allowed_relay_parents_under(&leaf, None), Some(&expected_ancestry[..]));
				assert_eq!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_A)), Some(&expected_ancestry[..]));

				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_B)).unwrap().is_empty());
				assert!(view.known_allowed_relay_parents_under(&leaf, Some(PARA_C)).unwrap().is_empty());
			}
		);
	}

	#[test]
	fn reuse_block_info_storage() {
		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

		let mut view = View::default();

		const PARA_A_MIN_PARENT: u32 = 1;
		let leaf_a_number = 3;
		let leaf_a = CHAIN_B[leaf_a_number - 1];
		let min_min_idx = (PARA_A_MIN_PARENT - GENESIS_NUMBER - 1) as usize;

		let prospective_response = vec![(PARA_A, PARA_A_MIN_PARENT)];

		let fut = view.activate_leaf(ctx.sender(), leaf_a).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(
				&mut ctx_handle,
				CHAIN_B,
				&CHAIN_B[(leaf_a_number - 1)..leaf_a_number],
			)
			.await;
			assert_min_relay_parents_request(&mut ctx_handle, &leaf_a, prospective_response).await;
			assert_block_header_requests(
				&mut ctx_handle,
				CHAIN_B,
				&CHAIN_B[min_min_idx..(leaf_a_number - 1)],
			)
			.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		// Blocks up to the 3rd are present in storage.
		const PARA_B_MIN_PARENT: u32 = 2;
		let leaf_b_number = 5;
		let leaf_b = CHAIN_B[leaf_b_number - 1];

		let prospective_response = vec![(PARA_B, PARA_B_MIN_PARENT)];

		let fut = view.activate_leaf(ctx.sender(), leaf_b).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(
				&mut ctx_handle,
				CHAIN_B,
				&CHAIN_B[(leaf_b_number - 1)..leaf_b_number],
			)
			.await;
			assert_min_relay_parents_request(&mut ctx_handle, &leaf_b, prospective_response).await;
			assert_block_header_requests(
				&mut ctx_handle,
				CHAIN_B,
				&CHAIN_B[leaf_a_number..(leaf_b_number - 1)], // Note the expected range.
			)
			.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		// Allowed relay parents for leaf A are preserved.
		let leaf_a_info =
			view.block_info_storage.get(&leaf_a).expect("block must be present in storage");
		assert_matches!(
			leaf_a_info.maybe_allowed_relay_parents,
			Some(ref allowed_relay_parents) => {
				assert_eq!(allowed_relay_parents.minimum_relay_parents[&PARA_A], PARA_A_MIN_PARENT);
				let expected_ancestry: Vec<Hash> =
					CHAIN_B[min_min_idx..leaf_a_number].iter().rev().copied().collect();
				let ancestry = view.known_allowed_relay_parents_under(&leaf_a, Some(PARA_A)).unwrap().to_vec();
				assert_eq!(ancestry, expected_ancestry);
			}
		);
	}

	#[test]
	fn pruning() {
		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

		let mut view = View::default();

		const PARA_A_MIN_PARENT: u32 = 3;
		let leaf_a = CHAIN_B.iter().rev().nth(1).unwrap();
		let leaf_a_idx = CHAIN_B.len() - 2;
		let min_a_idx = (PARA_A_MIN_PARENT - GENESIS_NUMBER - 1) as usize;

		let prospective_response = vec![(PARA_A, PARA_A_MIN_PARENT)];

		let fut = view
			.activate_leaf(ctx.sender(), *leaf_a)
			.timeout(TIMEOUT)
			.map(|res| res.unwrap().unwrap());
		let overseer_fut = async {
			assert_block_header_requests(
				&mut ctx_handle,
				CHAIN_B,
				&CHAIN_B[leaf_a_idx..(leaf_a_idx + 1)],
			)
			.await;
			assert_min_relay_parents_request(&mut ctx_handle, &leaf_a, prospective_response).await;
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &CHAIN_B[min_a_idx..leaf_a_idx])
				.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		// Also activate a leaf with a lesser minimum relay parent.
		const PARA_B_MIN_PARENT: u32 = 2;
		let leaf_b = CHAIN_B.last().unwrap();
		let min_b_idx = (PARA_B_MIN_PARENT - GENESIS_NUMBER - 1) as usize;

		let prospective_response = vec![(PARA_B, PARA_B_MIN_PARENT)];
		// Headers will be requested for the minimum block and the leaf.
		let blocks = &[CHAIN_B[min_b_idx], *leaf_b];

		let fut = view
			.activate_leaf(ctx.sender(), *leaf_b)
			.timeout(TIMEOUT)
			.map(|res| res.expect("`activate_leaf` timed out").unwrap());
		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &blocks[(blocks.len() - 1)..])
				.await;
			assert_min_relay_parents_request(&mut ctx_handle, &leaf_b, prospective_response).await;
			assert_block_header_requests(&mut ctx_handle, CHAIN_B, &blocks[..(blocks.len() - 1)])
				.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		// Prune implicit ancestor (no-op).
		let block_info_len = view.block_info_storage.len();
		view.deactivate_leaf(CHAIN_B[leaf_a_idx - 1]);
		assert_eq!(block_info_len, view.block_info_storage.len());

		// Prune a leaf with a greater minimum relay parent.
		view.deactivate_leaf(*leaf_b);
		for hash in CHAIN_B.iter().take(PARA_B_MIN_PARENT as usize) {
			assert!(!view.block_info_storage.contains_key(hash));
		}

		// Prune the last leaf.
		view.deactivate_leaf(*leaf_a);
		assert!(view.block_info_storage.is_empty());
	}

	#[test]
	fn genesis_ancestry() {
		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

		let mut view = View::default();

		const PARA_A_MIN_PARENT: u32 = 0;

		let prospective_response = vec![(PARA_A, PARA_A_MIN_PARENT)];
		let fut = view.activate_leaf(ctx.sender(), GENESIS_HASH).timeout(TIMEOUT).map(|res| {
			res.expect("`activate_leaf` timed out").unwrap();
		});
		let overseer_fut = async {
			assert_block_header_requests(&mut ctx_handle, &[GENESIS_HASH], &[GENESIS_HASH]).await;
			assert_min_relay_parents_request(&mut ctx_handle, &GENESIS_HASH, prospective_response)
				.await;
		};
		futures::executor::block_on(join(fut, overseer_fut));

		assert_matches!(
			view.known_allowed_relay_parents_under(&GENESIS_HASH, None),
			Some(hashes) if hashes == &[GENESIS_HASH]
		);
	}
}
