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

//! Tests of [`ChainSync`].

use super::*;
use crate::{
	block_relay_protocol::BlockResponseError, mock::MockBlockDownloader,
	service::network::NetworkServiceProvider,
};
use futures::{channel::oneshot::Canceled, executor::block_on};
use sc_block_builder::BlockBuilderBuilder;
use sc_network::RequestFailure;
use sc_network_common::sync::message::{BlockAnnounce, BlockData, BlockState, FromBlock};
use sp_blockchain::HeaderBackend;
use std::sync::Mutex;
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	BlockBuilderExt, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt, TestClient,
	TestClientBuilder, TestClientBuilderExt,
};

#[derive(Debug)]
struct ProxyBlockDownloader {
	protocol_name: ProtocolName,
	sender: std::sync::mpsc::Sender<BlockRequest<Block>>,
	request: Mutex<std::sync::mpsc::Receiver<BlockRequest<Block>>>,
}

#[async_trait::async_trait]
impl BlockDownloader<Block> for ProxyBlockDownloader {
	fn protocol_name(&self) -> &ProtocolName {
		&self.protocol_name
	}

	async fn download_blocks(
		&self,
		_who: PeerId,
		request: BlockRequest<Block>,
	) -> Result<Result<(Vec<u8>, ProtocolName), RequestFailure>, Canceled> {
		self.sender.send(request).unwrap();
		Ok(Ok((Vec::new(), self.protocol_name.clone())))
	}

	fn block_response_into_blocks(
		&self,
		_request: &BlockRequest<Block>,
		_response: Vec<u8>,
	) -> Result<Vec<BlockData<Block>>, BlockResponseError> {
		Ok(Vec::new())
	}
}

impl ProxyBlockDownloader {
	fn new(protocol_name: ProtocolName) -> Self {
		let (sender, receiver) = std::sync::mpsc::channel();
		Self { protocol_name, sender, request: Mutex::new(receiver) }
	}

	fn next_request(&self) -> BlockRequest<Block> {
		self.request.lock().unwrap().recv().unwrap()
	}
}

#[test]
fn processes_empty_response_on_justification_request_for_unknown_block() {
	// if we ask for a justification for a given block to a peer that doesn't know that block
	// (different from not having a justification), the peer will reply with an empty response.
	// internally we should process the response as the justification not being available.

	let client = Arc::new(TestClientBuilder::new().build());
	let peer_id = PeerId::random();

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		1,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let (a1_hash, a1_number) = {
		let a1 = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		(a1.hash(), *a1.header.number())
	};

	// add a new peer with the same best block
	sync.add_peer(peer_id, a1_hash, a1_number);

	// and request a justification for the block
	sync.request_justification(&a1_hash, a1_number);

	// the justification request should be scheduled to that peer
	assert!(sync
		.justification_requests()
		.iter()
		.any(|(who, request)| { *who == peer_id && request.from == FromBlock::Hash(a1_hash) }));

	// there are no extra pending requests
	assert_eq!(sync.extra_justifications.pending_requests().count(), 0);

	// there's one in-flight extra request to the expected peer
	assert!(sync.extra_justifications.active_requests().any(|(who, (hash, number))| {
		*who == peer_id && *hash == a1_hash && *number == a1_number
	}));

	// if the peer replies with an empty response (i.e. it doesn't know the block),
	// the active request should be cleared.
	sync.on_block_justification(peer_id, BlockResponse::<Block> { id: 0, blocks: vec![] })
		.unwrap();

	// there should be no in-flight requests
	assert_eq!(sync.extra_justifications.active_requests().count(), 0);

	// and the request should now be pending again, waiting for reschedule
	assert!(sync
		.extra_justifications
		.pending_requests()
		.any(|(hash, number)| { *hash == a1_hash && *number == a1_number }));
}

#[test]
fn restart_doesnt_affect_peers_downloading_finality_data() {
	let client = Arc::new(TestClientBuilder::new().build());

	// we request max 8 blocks to always initiate block requests to both peers for the test to be
	// deterministic
	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		1,
		8,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let peer_id2 = PeerId::random();
	let peer_id3 = PeerId::random();

	let new_blocks = |n| {
		for _ in 0..n {
			let block = BlockBuilderBuilder::new(&*client)
				.on_parent_block(client.chain_info().best_hash)
				.with_parent_block_number(client.chain_info().best_number)
				.build()
				.unwrap()
				.build()
				.unwrap()
				.block;
			block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
		}

		let info = client.info();
		(info.best_hash, info.best_number)
	};

	let (b1_hash, b1_number) = new_blocks(50);

	// add 2 peers at blocks that we don't have locally
	sync.add_peer(peer_id1, Hash::random(), 42);
	sync.add_peer(peer_id2, Hash::random(), 10);

	let network_provider = NetworkServiceProvider::new();
	let network_handle = network_provider.handle();

	// we wil send block requests to these peers
	// for these blocks we don't know about
	let actions = sync.actions(&network_handle).unwrap();
	assert_eq!(actions.len(), 2);
	assert!(actions.iter().all(|action| match action {
		SyncingAction::StartRequest { peer_id, .. } => peer_id == &peer_id1 || peer_id == &peer_id2,
		_ => false,
	}));

	// add a new peer at a known block
	sync.add_peer(peer_id3, b1_hash, b1_number);

	// we request a justification for a block we have locally
	sync.request_justification(&b1_hash, b1_number);

	// the justification request should be scheduled to the
	// new peer which is at the given block
	assert!(sync.justification_requests().iter().any(|(p, r)| {
		*p == peer_id3 &&
			r.fields == BlockAttributes::JUSTIFICATION &&
			r.from == FromBlock::Hash(b1_hash)
	}));

	assert_eq!(
		sync.peers.get(&peer_id3).unwrap().state,
		PeerSyncState::DownloadingJustification(b1_hash),
	);

	// drop old actions
	let _ = sync.take_actions();

	// we restart the sync state
	sync.restart();

	// which should make us cancel and send out again block requests to the first two peers
	let actions = sync.actions(&network_handle).unwrap();
	assert_eq!(actions.len(), 4);
	let mut cancelled_first = HashSet::new();
	assert!(actions.iter().all(|action| match action {
		SyncingAction::CancelRequest { peer_id, .. } => {
			cancelled_first.insert(peer_id);
			peer_id == &peer_id1 || peer_id == &peer_id2
		},
		SyncingAction::StartRequest { peer_id, .. } => {
			assert!(cancelled_first.remove(peer_id));
			peer_id == &peer_id1 || peer_id == &peer_id2
		},
		_ => false,
	}));

	// peer 3 should be unaffected as it was downloading finality data
	assert_eq!(
		sync.peers.get(&peer_id3).unwrap().state,
		PeerSyncState::DownloadingJustification(b1_hash),
	);

	// Set common block to something that we don't have (e.g. failed import)
	sync.peers.get_mut(&peer_id3).unwrap().common_number = 100;
	sync.restart();
	assert_eq!(sync.peers.get(&peer_id3).unwrap().common_number, 50);
}

/// Send a block announcement for the given `header`.
fn send_block_announce(header: Header, peer_id: PeerId, sync: &mut ChainSync<Block, TestClient>) {
	let announce = BlockAnnounce {
		header: header.clone(),
		state: Some(BlockState::Best),
		data: Some(Vec::new()),
	};

	let _ = sync.on_validated_block_announce(true, peer_id, &announce);
}

/// Create a block response from the given `blocks`.
fn create_block_response(blocks: Vec<Block>) -> BlockResponse<Block> {
	BlockResponse::<Block> {
		id: 0,
		blocks: blocks
			.into_iter()
			.map(|b| BlockData::<Block> {
				hash: b.hash(),
				header: Some(b.header().clone()),
				body: Some(b.deconstruct().1),
				indexed_body: None,
				receipt: None,
				message_queue: None,
				justification: None,
				justifications: None,
			})
			.collect(),
	}
}

/// Get a block request from `sync` and check that is matches the expected request.
fn get_block_request(
	sync: &mut ChainSync<Block, TestClient>,
	from: FromBlock<Hash, u64>,
	max: u32,
	peer: &PeerId,
) -> BlockRequest<Block> {
	let requests = sync.block_requests();

	log::trace!(target: LOG_TARGET, "Requests: {requests:?}");

	assert_eq!(1, requests.len());
	assert_eq!(*peer, requests[0].0);

	let request = requests[0].1.clone();

	assert_eq!(from, request.from);
	assert_eq!(Some(max), request.max);
	request
}

/// Build and import a new best block.
fn build_block(client: &TestClient, at: Option<Hash>, fork: bool) -> Block {
	let at = at.unwrap_or_else(|| client.info().best_hash);

	let mut block_builder = BlockBuilderBuilder::new(client)
		.on_parent_block(at)
		.fetch_parent_block_number(client)
		.unwrap()
		.build()
		.unwrap();

	if fork {
		block_builder.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6])).unwrap();
	}

	let block = block_builder.build().unwrap().block;

	block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
	block
}

fn unwrap_from_block_number(from: FromBlock<Hash, u64>) -> u64 {
	if let FromBlock::Number(from) = from {
		from
	} else {
		panic!("Expected a number!");
	}
}

/// A regression test for a behavior we have seen on a live network.
///
/// The scenario is that the node is doing a full resync and is connected to some node that is
/// doing a major sync as well. This other node that is doing a major sync will finish before
/// our node and send a block announcement message, but we don't have seen any block
/// announcement from this node in its sync process. Meaning our common number didn't change. It
/// is now expected that we start an ancestor search to find the common number.
#[test]
fn do_ancestor_search_when_common_block_to_best_queued_gap_is_to_big() {
	sp_tracing::try_init_simple();

	let blocks = {
		let client = TestClientBuilder::new().build();
		(0..MAX_DOWNLOAD_AHEAD * 2)
			.map(|_| build_block(&client, None, false))
			.collect::<Vec<_>>()
	};

	let client = Arc::new(TestClientBuilder::new().build());
	let info = client.info();

	let protocol_name = ProtocolName::Static("");
	let proxy_block_downloader = Arc::new(ProxyBlockDownloader::new(protocol_name.clone()));

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		protocol_name,
		proxy_block_downloader.clone(),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let peer_id2 = PeerId::random();

	let best_block = blocks.last().unwrap().clone();
	let max_blocks_to_request = sync.max_blocks_per_request;
	// Connect the node we will sync from
	sync.add_peer(peer_id1, best_block.hash(), *best_block.header().number());
	sync.add_peer(peer_id2, info.best_hash, 0);

	let mut best_block_num = 0;
	while best_block_num < MAX_DOWNLOAD_AHEAD {
		let request = get_block_request(
			&mut sync,
			FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
			max_blocks_to_request as u32,
			&peer_id1,
		);

		let from = unwrap_from_block_number(request.from.clone());

		let mut resp_blocks = blocks[best_block_num as usize..from as usize].to_vec();
		resp_blocks.reverse();

		let response = create_block_response(resp_blocks.clone());

		// Clear old actions to not deal with them
		let _ = sync.take_actions();

		sync.on_block_data(&peer_id1, Some(request), response).unwrap();

		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() == max_blocks_to_request as usize,
		));

		best_block_num += max_blocks_to_request as u32;

		let _ = sync.on_blocks_processed(
			max_blocks_to_request as usize,
			max_blocks_to_request as usize,
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id1),
						)),
						b.hash(),
					)
				})
				.collect(),
		);

		resp_blocks
			.into_iter()
			.rev()
			.for_each(|b| block_on(client.import_as_final(BlockOrigin::Own, b)).unwrap());
	}

	// "Wait" for the queue to clear
	sync.queue_blocks.clear();

	// Let peer2 announce that it finished syncing
	send_block_announce(best_block.header().clone(), peer_id2, &mut sync);

	// Populate actions with block requests from `block_requests()` (peer1) alongside the
	// ancestry search already queued for peer2.
	let block_requests = sync
		.block_requests()
		.into_iter()
		.map(|(peer_id, request)| sync.create_block_request_action(peer_id, request))
		.collect::<Vec<_>>();
	sync.actions.extend(block_requests);

	let actions = sync.take_actions().collect::<Vec<_>>();
	assert_eq!(actions.len(), 2);

	let (mut peer1_req, mut peer2_req) = (None, None);
	for action in actions {
		match action {
			SyncingAction::StartRequest { peer_id, request, .. } => {
				block_on(request).unwrap().unwrap();
				let req = proxy_block_downloader.next_request();
				if peer_id == peer_id1 {
					peer1_req = Some(req);
				} else if peer_id == peer_id2 {
					peer2_req = Some(req);
				} else {
					panic!("Unexpected peer: {peer_id}");
				}
			},
			action => panic!("Unexpected action: {}", action.name()),
		}
	}

	// We should now do an ancestor search to find the correct common block.
	let peer2_req = peer2_req.unwrap();
	assert_eq!(FromBlock::Number(best_block_num as u64), peer2_req.from);
	assert_eq!(Some(1), peer2_req.max);

	let response = create_block_response(vec![blocks[(best_block_num - 1) as usize].clone()]);

	sync.on_block_data(&peer_id2, Some(peer2_req), response).unwrap();

	let actions = sync.take_actions().collect::<Vec<_>>();
	assert!(actions.is_empty());

	let peer1_from = unwrap_from_block_number(peer1_req.unwrap().from);

	// As we are on the same chain, we should directly continue with requesting blocks from
	// peer 2 as well.
	get_block_request(
		&mut sync,
		FromBlock::Number(peer1_from + max_blocks_to_request as u64),
		max_blocks_to_request as u32,
		&peer_id2,
	);
}

/// A test that ensures that we can sync a huge fork.
///
/// The following scenario:
/// A peer connects to us and we both have the common block 512. The last finalized is 2048.
/// Our best block is 4096. The peer send us a block announcement with 4097 from a fork.
///
/// We will first do an ancestor search to find the common block. After that we start to sync
/// the fork and finish it ;)
#[test]
fn can_sync_huge_fork() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::new().build());
	let blocks = (0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 4)
		.map(|_| build_block(&client, None, false))
		.collect::<Vec<_>>();

	let fork_blocks = {
		let client = TestClientBuilder::new().build();
		let fork_blocks = blocks[..MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2]
			.into_iter()
			.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
			.cloned()
			.collect::<Vec<_>>();

		fork_blocks
			.into_iter()
			.chain(
				(0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 2 + 1).map(|_| build_block(&client, None, true)),
			)
			.collect::<Vec<_>>()
	};

	let info = client.info();

	let protocol_name = ProtocolName::Static("");
	let proxy_block_downloader = Arc::new(ProxyBlockDownloader::new(protocol_name.clone()));

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		protocol_name,
		proxy_block_downloader.clone(),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let finalized_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2 - 1].clone();
	let just = (*b"TEST", Vec::new());
	client.finalize_block(finalized_block.hash(), Some(just)).unwrap();
	sync.update_chain_info(&info.best_hash, info.best_number);

	let peer_id1 = PeerId::random();

	let common_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize / 2].clone();
	// Connect the node we will sync from
	sync.add_peer(peer_id1, common_block.hash(), *common_block.header().number());

	send_block_announce(fork_blocks.last().unwrap().header().clone(), peer_id1, &mut sync);

	// The announce triggers an ancestry search via actions
	let mut actions = sync.take_actions().collect::<Vec<_>>();
	assert_eq!(actions.len(), 1);
	let mut request = match actions.pop().unwrap() {
		SyncingAction::StartRequest { request, .. } => {
			block_on(request).unwrap().unwrap();
			proxy_block_downloader.next_request()
		},
		action => panic!("Unexpected action: {}", action.name()),
	};
	assert_eq!(FromBlock::Number(info.best_number), request.from);
	assert_eq!(Some(1), request.max);

	// Do the ancestor search
	loop {
		let block = &fork_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
		let response = create_block_response(vec![block.clone()]);

		sync.on_block_data(&peer_id1, Some(request.clone()), response).unwrap();

		let mut actions = sync.take_actions().collect::<Vec<_>>();

		request = if actions.is_empty() {
			// We found the ancestor
			break;
		} else {
			assert_eq!(actions.len(), 1);
			match actions.pop().unwrap() {
				SyncingAction::StartRequest { request, .. } => {
					block_on(request).unwrap().unwrap();
					proxy_block_downloader.next_request()
				},
				action => panic!("Unexpected action: {}", action.name()),
			}
		};

		log::trace!(target: LOG_TARGET, "Request: {request:?}");
	}

	// Now request and import the fork.
	let mut best_block_num = *finalized_block.header().number() as u32;
	let max_blocks_to_request = sync.max_blocks_per_request;
	while best_block_num < *fork_blocks.last().unwrap().header().number() as u32 - 1 {
		let request = get_block_request(
			&mut sync,
			FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
			max_blocks_to_request as u32,
			&peer_id1,
		);

		let from = unwrap_from_block_number(request.from.clone());

		let mut resp_blocks = fork_blocks[best_block_num as usize..from as usize].to_vec();
		resp_blocks.reverse();

		let response = create_block_response(resp_blocks.clone());

		sync.on_block_data(&peer_id1, Some(request), response).unwrap();

		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() == sync.max_blocks_per_request as usize
		));

		best_block_num += sync.max_blocks_per_request as u32;

		sync.on_blocks_processed(
			max_blocks_to_request as usize,
			max_blocks_to_request as usize,
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id1),
						)),
						b.hash(),
					)
				})
				.collect(),
		);

		// Discard pending actions
		let _ = sync.take_actions();

		resp_blocks
			.into_iter()
			.rev()
			.for_each(|b| block_on(client.import(BlockOrigin::Own, b)).unwrap());
	}

	// Request the tip
	get_block_request(&mut sync, FromBlock::Hash(fork_blocks.last().unwrap().hash()), 1, &peer_id1);
}

#[test]
fn syncs_fork_without_duplicate_requests() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::new().build());
	let blocks = (0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 4)
		.map(|_| build_block(&client, None, false))
		.collect::<Vec<_>>();

	let fork_blocks = {
		let client = TestClientBuilder::new().build();
		let fork_blocks = blocks[..MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2]
			.into_iter()
			.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
			.cloned()
			.collect::<Vec<_>>();

		fork_blocks
			.into_iter()
			.chain(
				(0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 2 + 1).map(|_| build_block(&client, None, true)),
			)
			.collect::<Vec<_>>()
	};

	let info = client.info();

	let protocol_name = ProtocolName::Static("");
	let proxy_block_downloader = Arc::new(ProxyBlockDownloader::new(protocol_name.clone()));

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		protocol_name,
		proxy_block_downloader.clone(),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let finalized_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2 - 1].clone();
	let just = (*b"TEST", Vec::new());
	client.finalize_block(finalized_block.hash(), Some(just)).unwrap();
	sync.update_chain_info(&info.best_hash, info.best_number);

	let peer_id1 = PeerId::random();

	let common_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize / 2].clone();
	// Connect the node we will sync from
	sync.add_peer(peer_id1, common_block.hash(), *common_block.header().number());

	send_block_announce(fork_blocks.last().unwrap().header().clone(), peer_id1, &mut sync);

	// The announce triggers an ancestry search via actions
	let mut actions = sync.take_actions().collect::<Vec<_>>();
	assert_eq!(actions.len(), 1);
	let mut request = match actions.pop().unwrap() {
		SyncingAction::StartRequest { request, .. } => {
			block_on(request).unwrap().unwrap();
			proxy_block_downloader.next_request()
		},
		action => panic!("Unexpected action: {}", action.name()),
	};
	assert_eq!(FromBlock::Number(info.best_number), request.from);
	assert_eq!(Some(1), request.max);

	// Do the ancestor search
	loop {
		let block = &fork_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
		let response = create_block_response(vec![block.clone()]);

		sync.on_block_data(&peer_id1, Some(request), response).unwrap();

		let mut actions = sync.take_actions().collect::<Vec<_>>();

		request = if actions.is_empty() {
			// We found the ancestor
			break;
		} else {
			assert_eq!(actions.len(), 1);
			match actions.pop().unwrap() {
				SyncingAction::StartRequest { request, .. } => {
					block_on(request).unwrap().unwrap();
					proxy_block_downloader.next_request()
				},
				action => panic!("Unexpected action: {}", action.name()),
			}
		};

		log::trace!(target: LOG_TARGET, "Request: {request:?}");
	}

	// Now request and import the fork.
	let mut best_block_num = *finalized_block.header().number() as u32;
	let max_blocks_to_request = sync.max_blocks_per_request;

	let mut request = get_block_request(
		&mut sync,
		FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
		max_blocks_to_request as u32,
		&peer_id1,
	);
	let last_block_num = *fork_blocks.last().unwrap().header().number() as u32 - 1;
	while best_block_num < last_block_num {
		let from = unwrap_from_block_number(request.from.clone());

		let mut resp_blocks = fork_blocks[best_block_num as usize..from as usize].to_vec();
		resp_blocks.reverse();

		let response = create_block_response(resp_blocks.clone());

		// Discard old actions
		let _ = sync.take_actions();

		sync.on_block_data(&peer_id1, Some(request.clone()), response).unwrap();

		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() == max_blocks_to_request as usize
		));

		best_block_num += max_blocks_to_request as u32;

		if best_block_num < last_block_num {
			// make sure we're not getting a duplicate request in the time before the blocks are
			// processed
			request = get_block_request(
				&mut sync,
				FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
				max_blocks_to_request as u32,
				&peer_id1,
			);
		}

		let mut notify_imported: Vec<_> = resp_blocks
			.iter()
			.rev()
			.map(|b| {
				(
					Ok(BlockImportStatus::ImportedUnknown(
						*b.header().number(),
						Default::default(),
						Some(peer_id1),
					)),
					b.hash(),
				)
			})
			.collect();

		// The import queue may send notifications in batches of varying size. So we simulate
		// this here by splitting the batch into 2 notifications.
		let max_blocks_to_request = sync.max_blocks_per_request;
		let second_batch = notify_imported.split_off(notify_imported.len() / 2);
		let _ = sync.on_blocks_processed(
			max_blocks_to_request as usize,
			max_blocks_to_request as usize,
			notify_imported,
		);

		let _ = sync.on_blocks_processed(
			max_blocks_to_request as usize,
			max_blocks_to_request as usize,
			second_batch,
		);

		resp_blocks
			.into_iter()
			.rev()
			.for_each(|b| block_on(client.import(BlockOrigin::Own, b)).unwrap());
	}

	// Request the tip
	get_block_request(&mut sync, FromBlock::Hash(fork_blocks.last().unwrap().hash()), 1, &peer_id1);
}

#[test]
fn removes_target_fork_on_disconnect() {
	sp_tracing::try_init_simple();
	let client = Arc::new(TestClientBuilder::new().build());
	let blocks = (0..3).map(|_| build_block(&client, None, false)).collect::<Vec<_>>();

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		1,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let common_block = blocks[1].clone();
	// Connect the node we will sync from
	sync.add_peer(peer_id1, common_block.hash(), *common_block.header().number());

	// Create a "new" header and announce it
	let mut header = blocks[0].header().clone();
	header.number = 4;
	send_block_announce(header, peer_id1, &mut sync);
	assert!(sync.fork_targets.len() == 1);

	let _ = sync.remove_peer(&peer_id1);
	assert!(sync.fork_targets.len() == 0);
}

#[test]
fn can_import_response_with_missing_blocks() {
	sp_tracing::try_init_simple();
	let client2 = TestClientBuilder::new().build();
	let blocks = (0..4).map(|_| build_block(&client2, None, false)).collect::<Vec<_>>();

	let empty_client = Arc::new(TestClientBuilder::new().build());

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		empty_client.clone(),
		1,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let best_block = blocks[3].clone();
	sync.add_peer(peer_id1, best_block.hash(), *best_block.header().number());

	sync.peers.get_mut(&peer_id1).unwrap().state = PeerSyncState::Available;
	sync.peers.get_mut(&peer_id1).unwrap().common_number = 0;

	// Request all missing blocks and respond only with some.
	let request = get_block_request(&mut sync, FromBlock::Hash(best_block.hash()), 4, &peer_id1);
	let response =
		create_block_response(vec![blocks[3].clone(), blocks[2].clone(), blocks[1].clone()]);
	sync.on_block_data(&peer_id1, Some(request.clone()), response).unwrap();
	assert_eq!(sync.best_queued_number, 0);

	// Request should only contain the missing block.
	let request = get_block_request(&mut sync, FromBlock::Number(1), 1, &peer_id1);
	let response = create_block_response(vec![blocks[0].clone()]);
	sync.on_block_data(&peer_id1, Some(request), response).unwrap();
	assert_eq!(sync.best_queued_number, 4);
}
#[test]
fn ancestor_search_repeat() {
	let state = AncestorSearchState::<Block>::BinarySearch(1, 3);
	assert!(handle_ancestor_search_state(&state, 2, true).is_none());
}

#[test]
fn sync_restart_removes_block_but_not_justification_requests() {
	let client = Arc::new(TestClientBuilder::new().build());
	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		1,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peers = vec![PeerId::random(), PeerId::random()];

	let new_blocks = |n| {
		for _ in 0..n {
			let block = BlockBuilderBuilder::new(&*client)
				.on_parent_block(client.chain_info().best_hash)
				.with_parent_block_number(client.chain_info().best_number)
				.build()
				.unwrap()
				.build()
				.unwrap()
				.block;
			block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
		}

		let info = client.info();
		(info.best_hash, info.best_number)
	};

	let (b1_hash, b1_number) = new_blocks(50);

	// add new peer and request blocks from them
	sync.add_peer(peers[0], Hash::random(), 42);

	// we don't actually perform any requests, just keep track of peers waiting for a response
	let mut pending_responses = HashSet::new();

	// we wil send block requests to these peers
	// for these blocks we don't know about
	for (peer, _request) in sync.block_requests() {
		// "send" request
		pending_responses.insert(peer);
	}

	// add a new peer at a known block
	sync.add_peer(peers[1], b1_hash, b1_number);

	// we request a justification for a block we have locally
	sync.request_justification(&b1_hash, b1_number);

	// the justification request should be scheduled to the
	// new peer which is at the given block
	let mut requests = sync.justification_requests();
	assert_eq!(requests.len(), 1);
	let (peer, _request) = requests.remove(0);
	// "send" request
	assert!(pending_responses.insert(peer));

	assert!(!std::matches!(
		sync.peers.get(&peers[0]).unwrap().state,
		PeerSyncState::DownloadingJustification(_),
	));
	assert_eq!(
		sync.peers.get(&peers[1]).unwrap().state,
		PeerSyncState::DownloadingJustification(b1_hash),
	);
	assert_eq!(pending_responses.len(), 2);

	// discard old actions
	let _ = sync.take_actions();

	// restart sync
	sync.restart();
	let actions = sync.take_actions().collect::<Vec<_>>();
	for action in actions.iter() {
		match action {
			SyncingAction::CancelRequest { peer_id, key: _ } => {
				pending_responses.remove(&peer_id);
			},
			SyncingAction::StartRequest { peer_id, .. } => {
				// we drop obsolete response, but don't register a new request, it's checked in
				// the `assert!` below
				pending_responses.remove(&peer_id);
			},
			action @ _ => panic!("Unexpected action: {}", action.name()),
		}
	}
	assert!(actions.iter().any(|action| {
		match action {
			SyncingAction::StartRequest { peer_id, .. } => peer_id == &peers[0],
			_ => false,
		}
	}));

	assert_eq!(pending_responses.len(), 1);
	assert!(pending_responses.contains(&peers[1]));
	assert_eq!(
		sync.peers.get(&peers[1]).unwrap().state,
		PeerSyncState::DownloadingJustification(b1_hash),
	);
	let _ = sync.remove_peer(&peers[1]);
	pending_responses.remove(&peers[1]);
	assert_eq!(pending_responses.len(), 0);
}

/// The test demonstrates https://github.com/paritytech/polkadot-sdk/issues/2094.
/// TODO: convert it into desired behavior test once the issue is fixed (see inline comments).
/// The issue: we currently rely on block numbers instead of block hash
/// to download blocks from peers. As a result, we can end up with blocks
/// from different forks as shown by the test.
#[test]
#[should_panic]
fn request_across_forks() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::new().build());
	let blocks = (0..100).map(|_| build_block(&client, None, false)).collect::<Vec<_>>();

	let fork_a_blocks = {
		let client = TestClientBuilder::new().build();
		let mut fork_blocks = blocks[..]
			.into_iter()
			.inspect(|b| {
				assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
				block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap()
			})
			.cloned()
			.collect::<Vec<_>>();
		for _ in 0..10 {
			fork_blocks.push(build_block(&client, None, false));
		}
		fork_blocks
	};

	let fork_b_blocks = {
		let client = TestClientBuilder::new().build();
		let mut fork_blocks = blocks[..]
			.into_iter()
			.inspect(|b| {
				assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
				block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap()
			})
			.cloned()
			.collect::<Vec<_>>();
		for _ in 0..10 {
			fork_blocks.push(build_block(&client, None, true));
		}
		fork_blocks
	};

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	// Add the peers, all at the common ancestor 100.
	let common_block = blocks.last().unwrap();
	let peer_id1 = PeerId::random();
	sync.add_peer(peer_id1, common_block.hash(), *common_block.header().number());
	let peer_id2 = PeerId::random();
	sync.add_peer(peer_id2, common_block.hash(), *common_block.header().number());

	// Peer 1 announces 107 from fork 1, 100-107 get downloaded.
	{
		let block = (&fork_a_blocks[106]).clone();
		let peer = peer_id1;
		log::trace!(target: LOG_TARGET, "<1> {peer} announces from fork 1");
		send_block_announce(block.header().clone(), peer, &mut sync);
		let request = get_block_request(&mut sync, FromBlock::Hash(block.hash()), 7, &peer);
		let mut resp_blocks = fork_a_blocks[100_usize..107_usize].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());

		// Drop old actions
		let _ = sync.take_actions();

		sync.on_block_data(&peer, Some(request), response).unwrap();
		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() == 7_usize
		));
		assert_eq!(sync.best_queued_number, 107);
		assert_eq!(sync.best_queued_hash, block.hash());
		assert!(sync.is_known(&block.header.parent_hash()));
	}

	// Peer 2 also announces 107 from fork 1.
	{
		let prev_best_number = sync.best_queued_number;
		let prev_best_hash = sync.best_queued_hash;
		let peer = peer_id2;
		log::trace!(target: LOG_TARGET, "<2> {peer} announces from fork 1");
		for i in 100..107 {
			let block = (&fork_a_blocks[i]).clone();
			send_block_announce(block.header().clone(), peer, &mut sync);
			assert!(sync.block_requests().is_empty());
		}
		assert_eq!(sync.best_queued_number, prev_best_number);
		assert_eq!(sync.best_queued_hash, prev_best_hash);
	}

	// Peer 2 undergoes reorg, announces 108 from fork 2, gets downloaded even though we
	// don't have the parent from fork 2.
	{
		let block = (&fork_b_blocks[107]).clone();
		let peer = peer_id2;
		log::trace!(target: LOG_TARGET, "<3> {peer} announces from fork 2");
		send_block_announce(block.header().clone(), peer, &mut sync);
		// TODO: when the issue is fixed, this test can be changed to test the
		// expected behavior instead. The needed changes would be:
		// 1. Remove the `#[should_panic]` directive
		// 2. These should be changed to check that sync.block_requests().is_empty(), after the
		//    block is announced.
		let request = get_block_request(&mut sync, FromBlock::Hash(block.hash()), 1, &peer);
		let response = create_block_response(vec![block.clone()]);

		// Drop old actions we are not going to check
		let _ = sync.take_actions();

		sync.on_block_data(&peer, Some(request), response).unwrap();
		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() == 1_usize
		));
		assert!(sync.is_known(&block.header.parent_hash()));
	}
}

/// This test simulates a scenario where we get a `VerificationFailed` error
/// while a gap reported by our client.info(). Then the gap is filled after
/// the restart of the sync process. The test ensures that the gap is properly closed
/// on importing unknown blocks (ie blocks we don't have in our chain yet).
#[test]
fn sync_verification_failed_with_gap_filled() {
	sp_tracing::try_init_simple();

	// We only care about 2 iterations of the loop (since max blocks per request is 64).
	const TEST_TARGET: u32 = 64 * 3;

	let blocks = {
		let client = TestClientBuilder::new().build();
		(0..TEST_TARGET).map(|_| build_block(&client, None, false)).collect::<Vec<_>>()
	};

	let client = Arc::new(TestClientBuilder::new().build());
	let info = client.info();

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let peer_id2 = PeerId::random();

	let best_block = blocks.last().unwrap().clone();
	let max_blocks_to_request = sync.max_blocks_per_request;

	let status = sync.status();
	assert!(status.warp_sync.is_none());
	log::info!(target: LOG_TARGET, "Before adding peers: {status:?}");

	// Connect the node we will sync from
	sync.add_peer(peer_id1, best_block.hash(), *best_block.header().number());
	sync.add_peer(peer_id2, info.best_hash, 0);

	let mut best_block_num = 0;
	assert_eq!(sync.best_queued_number, 0);

	// Two iterations to simulate the gap filling.
	for loop_index in 0..2 {
		log::info!(target: LOG_TARGET, "Loop index: {loop_index}");

		// Build the request.
		let request = get_block_request(
			&mut sync,
			FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
			max_blocks_to_request as u32,
			&peer_id1,
		);
		let from = unwrap_from_block_number(request.from.clone());
		let mut resp_blocks = blocks[best_block_num as usize..from as usize].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());

		// Clear old actions to not deal with them
		let _ = sync.take_actions();

		let status = sync.status();
		log::info!(target: LOG_TARGET, "Status before on_block_data: {status:?}");

		sync.on_block_data(&peer_id1, Some(request.clone()), response.clone()).unwrap();

		let actions = sync.take_actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			&actions[0],
			SyncingAction::ImportBlocks{ origin: _, blocks } if blocks.len() ==
		max_blocks_to_request as usize, ));

		let status = sync.status();
		log::info!(target: LOG_TARGET, "Status before processing blocks: {status:?}");

		best_block_num += max_blocks_to_request as u32;

		let responses: Vec<_> = resp_blocks
			.iter()
			.rev()
			.map(|b| {
				(
					Ok(BlockImportStatus::ImportedUnknown(
						*b.header().number(),
						Default::default(),
						Some(peer_id1),
					)),
					b.hash(),
				)
			})
			.collect();

		sync.on_blocks_processed(
			max_blocks_to_request as usize,
			max_blocks_to_request as usize,
			responses,
		);

		let status = sync.status();
		log::info!(target: LOG_TARGET, "Status after processing blocks: {status:?}");

		// Import the blocks as final to the client.
		resp_blocks
			.into_iter()
			.rev()
			.for_each(|b| block_on(client.import_as_final(BlockOrigin::Own, b)).unwrap());

		if loop_index == 0 {
			log::info!(target: LOG_TARGET, "Peer state {:#?}", sync.peers);

			// Both peers are in the available state.
			match sync.peers.get(&peer_id1) {
				Some(peer) => assert_eq!(peer.state, PeerSyncState::Available),
				None => panic!("Peer not found"),
			}
			match sync.peers.get(&peer_id2) {
				Some(peer) => assert_eq!(peer.state, PeerSyncState::Available),
				None => panic!("Peer not found"),
			}

			// Simulate that we encounter a `VerificationFailed` error while processing the blocks.
			// During this error, the sync will enter the `AncestorSearch` state for the peer 1
			// because of the sync restart operation. Then, the peer will be in the `Available`
			// state after the ancestor search is done. However, we still have the gap present.
			sync.gap_sync = Some(GapSync {
				best_queued_number: 64 as u64,
				target: 84 as u64,
				blocks: BlockCollection::new(),
				stats: GapSyncStats::new(),
			});
		} else if loop_index == 1 {
			if sync.gap_sync.is_none() {
				log::info!(target: LOG_TARGET, "Gap successfully closed");
			} else {
				panic!("Gap not closed after the second loop");
			}
		}
	}
}

#[test]
fn sync_gap_filled_regardless_of_blocks_origin() {
	sp_tracing::try_init_simple();

	let blocks = {
		let client = TestClientBuilder::new().build();
		(0..2).map(|_| build_block(&client, None, false)).collect::<Vec<_>>()
	};

	let client = Arc::new(TestClientBuilder::new().build());
	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();

	// BlockImportStatus::ImportedUnknown clears the gap.
	{
		// Simulate that we encounter a `VerificationFailed` error while processing the blocks
		// and the client.info() reports a gap.
		sync.gap_sync = Some(GapSync {
			best_queued_number: *blocks[0].header().number(),
			target: *blocks[0].header().number(),
			blocks: BlockCollection::new(),
			stats: GapSyncStats::new(),
		});

		// Announce the block as unknown.
		let results = [(
			Ok(BlockImportStatus::ImportedUnknown(
				*blocks[0].header().number(),
				Default::default(),
				Some(peer_id1),
			)),
			blocks[0].hash(),
		)];
		sync.on_blocks_processed(1, 1, results.into_iter().collect());
		// Ensure the gap is cleared out.
		assert!(sync.gap_sync.is_none());
	}

	// BlockImportStatus::ImportedKnown also clears the gap.
	{
		sync.gap_sync = Some(GapSync {
			best_queued_number: *blocks[0].header().number(),
			target: *blocks[0].header().number(),
			blocks: BlockCollection::new(),
			stats: GapSyncStats::new(),
		});

		// Announce the block as known.
		let results = [(
			Ok(BlockImportStatus::ImportedKnown(*blocks[0].header().number(), Some(peer_id1))),
			blocks[0].hash(),
		)];

		sync.on_blocks_processed(1, 1, results.into_iter().collect());
		// Ensure the gap is cleared out.
		assert!(sync.gap_sync.is_none());
	}
}

#[test]
fn gap_sync_body_request_depends_on_pruning_mode() {
	sp_tracing::try_init_simple();

	for archive_blocks in [true, false] {
		// Bodies only needed for archive mode
		let should_request_bodies = archive_blocks;
		log::info!("Testing gap sync with archive_blocks: {}", archive_blocks);

		let client = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..10).map(|_| build_block(&client, None, false)).collect::<Vec<_>>();

		let mut sync = ChainSync::new(
			ChainSyncMode::Full,
			client.clone(),
			5,
			64,
			ProtocolName::Static(""),
			Arc::new(MockBlockDownloader::new()),
			archive_blocks,
			None,
			std::iter::empty(),
		)
		.unwrap();

		let peer_id = PeerId::random();

		// Simulate gap: blocks 5-10 missing
		sync.gap_sync = Some(GapSync {
			best_queued_number: 5,
			target: 10,
			blocks: BlockCollection::new(),
			stats: GapSyncStats::new(),
		});

		sync.add_peer(peer_id, blocks[9].hash(), 10);

		let requests = sync.block_requests();
		assert!(
			!requests.is_empty(),
			"[archive_blocks={archive_blocks}] Should generate gap sync request"
		);

		let (_peer, request) = &requests[0];

		// Verify the exact expected field combination
		let expected_fields = if should_request_bodies {
			BlockAttributes::HEADER | BlockAttributes::BODY | BlockAttributes::JUSTIFICATION
		} else {
			BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION
		};

		assert_eq!(
			request.fields, expected_fields,
			"[archive_blocks={archive_blocks}] Gap sync fields mismatch: expected {expected_fields:?}, got {:?}",
			request.fields
		);
	}
}

#[test]
fn regular_sync_always_requests_bodies_regardless_of_pruning() {
	sp_tracing::try_init_simple();

	// Verify that regular (non-gap) sync always requests bodies,
	// regardless of pruning mode - our optimization only applies to gap sync
	for archive_blocks in [true, false] {
		log::info!("Testing regular sync with archive_blocks: {}", archive_blocks);

		let client = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..5).map(|_| build_block(&client, None, false)).collect::<Vec<_>>();

		let mut sync = ChainSync::new(
			ChainSyncMode::Full,
			client.clone(),
			5,
			64,
			ProtocolName::Static(""),
			Arc::new(MockBlockDownloader::new()),
			archive_blocks,
			None,
			std::iter::empty(),
		)
		.unwrap();

		let peer_id = PeerId::random();

		// Ensure we're NOT in gap sync mode
		assert!(
			sync.gap_sync.is_none(),
			"[archive_blocks={archive_blocks}] Should not have gap sync active"
		);

		// Add peer ahead of us to trigger regular sync
		sync.add_peer(peer_id, blocks[4].hash(), 5);

		let requests = sync.block_requests();

		// Regular sync may not always generate requests immediately depending on state,
		// but when it does, it should request bodies
		if !requests.is_empty() {
			let (_peer, request) = &requests[0];

			// Verify exact expected fields for Full mode
			let expected_fields =
				BlockAttributes::HEADER | BlockAttributes::BODY | BlockAttributes::JUSTIFICATION;

			assert_eq!(
				request.fields, expected_fields,
				"[archive_blocks={archive_blocks}] Regular sync fields mismatch: expected {expected_fields:?}, got {:?}",
				request.fields
			);
		}
	}
}

/// During major sync, peers that announce blocks on unknown forks should NOT be put into
/// `AncestorSearch`. The scenario:
///
/// 1. We import some blocks, then peers connect that are far ahead.
/// 2. The import queue fills up, so `add_peer_inner` skips ancestry search and adds new peers as
///    `Available` with `common_number = best_queued_number`.
/// 3. One of those peers announces a new best block on an unknown fork.
/// 4. `continues_known_fork` is false, triggering ancestry search.
/// 5. The peer loses its `allowed_requests` token and can no longer serve block downloads.
///
/// If this happens to enough peers, sync stalls.
#[test]
fn no_ancestry_search_during_major_sync() {
	sp_tracing::try_init_simple();

	let (blocks, fork_block) = {
		let client = TestClientBuilder::new().build();
		let blocks = (0..MAX_DOWNLOAD_AHEAD * 2)
			.map(|_| build_block(&client, None, false))
			.collect::<Vec<_>>();

		let fork_block = build_block(&client, Some(blocks[blocks.len() - 2].hash()), true);
		(blocks, fork_block)
	};

	let client = Arc::new(TestClientBuilder::new().build());

	// Import a few blocks so we're NOT at genesis (add_peer skips ancestry search at genesis).
	for b in &blocks[..10] {
		block_on(client.import(BlockOrigin::Own, b.clone())).unwrap();
	}

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		5,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	let peer_id1 = PeerId::random();
	let peer_id2 = PeerId::random();

	let best_block = blocks.last().unwrap().clone();
	let best_block_num = *best_block.header().number();

	// peer1 is far ahead — triggers ancestry search since queue is empty.
	sync.add_peer(peer_id1, best_block.hash(), best_block_num);
	assert!(matches!(
		sync.peers.get(&peer_id1).unwrap().state,
		PeerSyncState::AncestorSearch { .. }
	));

	// Fill the import queue to trigger the "skip ancestry search" path in add_peer_inner.
	for block in &blocks[..MAJOR_SYNC_BLOCKS as usize + 1] {
		sync.queue_blocks.insert(block.hash());
	}

	// peer2 is added — should be `Available` because queue_blocks > MAJOR_SYNC_BLOCKS.
	sync.add_peer(peer_id2, best_block.hash(), best_block_num);
	assert_eq!(sync.peers.get(&peer_id2).unwrap().state, PeerSyncState::Available);

	// peer2 announces a new best block whose parent is NOT peer.best_hash.
	// Sanity: the fork block's parent is NOT best_block.hash()
	assert_ne!(fork_block.header().parent_hash(), &best_block.hash());

	let announce = BlockAnnounce {
		header: fork_block.header().clone(),
		state: Some(BlockState::Best),
		data: Some(Vec::new()),
	};

	let _ = sync.on_validated_block_announce(true, peer_id2, &announce);

	// peer2 should NOT be in AncestorSearch during major sync — it should stay Available.
	assert!(
		!matches!(sync.peers.get(&peer_id2).unwrap().state, PeerSyncState::AncestorSearch { .. }),
		"Peer should not be in AncestorSearch during major sync — this would stall sync!",
	);

	// No ancestry search action should be queued for peer2.
	let actions = sync.take_actions().collect::<Vec<_>>();
	for action in &actions {
		if let SyncingAction::StartRequest { peer_id, .. } = action {
			assert_ne!(*peer_id, peer_id2, "No request should be sent to peer2 during major sync",);
		}
	}
}

/// Regression test for <https://github.com/paritytech/polkadot-sdk/issues/12364>.
///
/// Reproduces a real production stall observed on `summit-people-rpc-node-2` on 2026-06-12:
/// after a backward reorg the node's `best_queued_hash` is left pointing at a block that the
/// client has pruned, while the canonical block at the same height has a *different* hash. The
/// node never asks any peer for that canonical block, so sync silently stops forever (0 bps).
///
/// The mechanism (verified against the production logs, see `logs/rpc2-reorg-timeline.txt`):
///
/// 1. The node downloads Branch A and ends up with `best_queued_number = N+1`,
///    `best_queued_hash = H_A_{N+1}`. `on_block_queued` drove every peer's
///    `common_number` up to `N+1` (chain_sync.rs:1762-1778).
/// 2. Branch B blocks arrive as non-best announcements and the client imports them.
/// 3. The relay-chain finalizes Branch B at height `N+1` (different hash, `H_B_{N+1}`),
///    which pulls the client's best+finalized backward and prunes `H_A_{N+1}`.
/// 4. `on_block_finalized` is a no-op for the download layer (chain_sync.rs:899-921),
///    so `best_queued_hash` stays stuck on the pruned `H_A_{N+1}` and every peer's
///    `common_number` stays at `N+1` (monotonic-up only, chain_sync.rs:322-334).
/// 5. The block-request floor is `first_different = common + 1 = N+2` (blocks.rs:125),
///    so canonical `H_B_{N+1}` at height `N+1` sits *below* the floor and is never
///    requested. `fork_sync_request` also won't ask for it because the block is "known"
///    to the client (it imported it already).
/// 6. With no error in the import path, `restart()` / `reset_sync_start_point` is never
///    called, so `best_queued_hash` never reconciles with the client's real best.
///
/// This test recreates step 1-5 and asserts the divergence between `best_queued_hash`
/// and the client's real best hash, plus the fact that no peer is asked for the canonical
/// chain. It currently fails — the fix should make it pass.
#[test]
fn stuck_on_pruned_best_queued_after_backward_reorg_to_different_fork() {
	sp_tracing::try_init_simple();

	// === Build two forks that diverge at N=10 and both end at N+1=11 ===
	//
	//                          ┌── #11 H_A  (Branch A — what ChainSync downloads first)
	//                          │
	//   genesis ── … ── #10 ───┤
	//                          │
	//                          └── #11 H_B  (Branch B — finalized by the relay)
	//
	// `fork = false` for A, `fork = true` for B → distinct storage root → distinct hashes
	// at the same height.
	const COMMON_BLOCKS: usize = 10;
	let (common_blocks, branch_a_tip, branch_b_tip) = {
		let client = TestClientBuilder::new().build();
		let common = (0..COMMON_BLOCKS)
			.map(|_| build_block(&client, None, false))
			.collect::<Vec<_>>();
		let fork_parent = common.last().unwrap().hash();

		// Branch A tip at height #11 (no fork-marker change).
		let a_tip = build_block(&client, Some(fork_parent), false);

		// To produce a *different* block at the same height from the same parent, we have
		// to start from a fresh client (the existing one already has #11 H_A imported).
		// Rebuild the common chain on a second client, then build the Branch B tip there.
		let client_b = TestClientBuilder::new().build();
		for b in &common {
			block_on(client_b.import(BlockOrigin::Own, b.clone())).unwrap();
		}
		let b_tip = build_block(&client_b, Some(fork_parent), true);

		assert_eq!(*a_tip.header().number(), (COMMON_BLOCKS as u64) + 1);
		assert_eq!(*b_tip.header().number(), (COMMON_BLOCKS as u64) + 1);
		assert_ne!(a_tip.hash(), b_tip.hash(), "Branches must diverge at the tip");
		(common, a_tip, b_tip)
	};

	// === Set up the node-under-test's client and ChainSync ===
	let client = Arc::new(TestClientBuilder::new().build());

	let mut sync = ChainSync::new(
		ChainSyncMode::Full,
		client.clone(),
		1,
		64,
		ProtocolName::Static(""),
		Arc::new(MockBlockDownloader::new()),
		false,
		None,
		std::iter::empty(),
	)
	.unwrap();

	// === Step 1: peer1 (on Branch A's tip #11 H_A) downloads the full chain.
	// We start at genesis (best_queued = 0) so add_peer skips ancestor search and we
	// just download #1..#11 from peer1.
	let peer_id1 = PeerId::random();
	let peer_id2 = PeerId::random();
	sync.add_peer(peer_id1, branch_a_tip.hash(), *branch_a_tip.header().number());

	let req = get_block_request(
		&mut sync,
		FromBlock::Hash(branch_a_tip.hash()),
		(COMMON_BLOCKS + 1) as u32,
		&peer_id1,
	);
	let mut branch_a_chain: Vec<_> = common_blocks.clone();
	branch_a_chain.push(branch_a_tip.clone());
	let mut resp_blocks = branch_a_chain.clone();
	resp_blocks.reverse();
	let response = create_block_response(resp_blocks);
	let _ = sync.take_actions();
	sync.on_block_data(&peer_id1, Some(req), response).unwrap();
	let _ = sync.take_actions();

	// Import the whole Branch A into the client so the client agrees that H_A is best.
	for b in &branch_a_chain {
		block_on(client.import(BlockOrigin::Own, b.clone())).unwrap();
	}
	sync.on_blocks_processed(
		branch_a_chain.len(),
		branch_a_chain.len(),
		branch_a_chain
			.iter()
			.map(|b| {
				(
					Ok(BlockImportStatus::ImportedUnknown(
						*b.header().number(),
						Default::default(),
						Some(peer_id1),
					)),
					b.hash(),
				)
			})
			.collect(),
	);
	let _ = sync.take_actions();

	// ChainSync's view: best_queued = #11 / H_A. Client's view: best = #11 / H_A.
	assert_eq!(sync.best_queued_number, 11, "best_queued must have advanced to #11");
	assert_eq!(sync.best_queued_hash, branch_a_tip.hash(), "best_queued_hash must be H_A");
	assert_eq!(client.info().best_hash, branch_a_tip.hash());
	assert_eq!(sync.peers.get(&peer_id1).unwrap().common_number, 11);

	// Peer 2 joins on canonical Branch B. It will (re-)announce H_B later.
	sync.add_peer(peer_id2, branch_b_tip.hash(), *branch_b_tip.header().number());
	let _ = sync.take_actions();

	// === Step 3: now reality diverges from ChainSync's view.
	//
	// In production this is what happens: the relay-chain finalizes Branch B at #11. The
	// client adopts Branch B (different hash at the same height) and prunes Branch A's
	// #11 H_A. We simulate this by importing Branch B's tip *as final* — `import_as_final`
	// makes it both the client's best AND finalized.
	//
	// Note we never imported Branch A into the *real* client. After this step:
	//   client.info().best_hash      = H_B (Branch B tip)
	//   client.info().finalized_hash = H_B
	//   ChainSync.best_queued_hash   = H_A   ← still pointing at a hash the client does NOT have
	block_on(client.import_as_final(BlockOrigin::Own, branch_b_tip.clone())).unwrap();

	let info_after = client.info();
	assert_eq!(info_after.best_hash, branch_b_tip.hash());
	assert_eq!(info_after.finalized_hash, branch_b_tip.hash());
	assert_eq!(info_after.best_number, 11);
	assert_eq!(info_after.finalized_number, 11);

	// Notify ChainSync of the finalization — exactly what the client does in production.
	sync.on_block_finalized(&branch_b_tip.hash(), 11);

	// === Step 4: peer2 (on canonical Branch B) re-announces the canonical tip H_B.
	// In production this happens continually. The announcement must NOT trigger ancestry
	// search because the import queue / target makes the node still classed major-syncing,
	// and even if it did, the block is "known" to the client by now so the major-sync
	// guards on chain_sync.rs:549 / :587 keep it from being routed to fork_targets.
	send_block_announce(branch_b_tip.header().clone(), peer_id2, &mut sync);
	let _ = sync.take_actions();

	// === The bug: ChainSync is now stuck. Document it with three independent observations.

	// --- Observation A: best_queued_hash is stale and references a pruned block, while
	// the canonical block at the same height has a different hash.
	assert_eq!(sync.best_queued_number, 11);
	assert_eq!(
		sync.best_queued_hash,
		branch_a_tip.hash(),
		"BUG: best_queued_hash is still H_A even though the client moved to H_B",
	);
	let bq_in_client = client.block_status(sync.best_queued_hash).unwrap();
	assert!(
		matches!(bq_in_client, BlockStatus::InChainPruned),
		"BUG: best_queued_hash {:?} should be pruned by the reorg (got status={:?})",
		sync.best_queued_hash,
		bq_in_client,
	);

	// --- Observation B: ChainSync's view of "best queued" diverges from the client's real best.
	assert_ne!(
		sync.best_queued_hash,
		client.info().best_hash,
		"BUG: best_queued_hash diverged from client.info().best_hash — \
		 ChainSync no longer agrees with the client about the canonical chain at #11",
	);

	// --- Observation C: the recovery paths are dead. No peer is asked for canonical H_B,
	// and `fork_targets` is empty so `fork_sync_request` won't ask either.
	assert!(
		sync.fork_targets.is_empty(),
		"BUG: fork_targets is empty — the canonical block H_B was never registered as a fork \
		 target, so the only path that would request it by hash is dead",
	);

	let requests = sync.block_requests();
	let canonical_requested = requests
		.iter()
		.any(|(_, r)| matches!(r.from, FromBlock::Hash(h) if h == branch_b_tip.hash()));
	assert!(
		canonical_requested,
		"BUG: no peer is being asked for the canonical block H_B={:?} at height #11. \
		 block_requests()={:?}. ChainSync is stuck — the node will report 0 bps forever \
		 (issue #12364).",
		branch_b_tip.hash(),
		requests,
	);
}
