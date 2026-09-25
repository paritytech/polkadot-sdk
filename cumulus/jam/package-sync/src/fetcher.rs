// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Eager, best-effort fetching of the package info of every imported foreign block.
//!
//! The fetcher watches the client's import notifications, and for every block this node did not
//! author (and does not already know) it asks the collators of that block, author first, over the
//! package-info protocol. A block is only useful while it is close to the best block, so
//! blocks more than [`MAX_BEHIND_BEST`] behind the best are skipped.
//!
//! The JAM-specific part — rebuilding and verifying the work package from the response — is
//! delegated to [`PackageInfoAcceptor`].

use crate::types::{PackageInfo, PackageInfoRequest, PackageInfoResponse};
use async_trait::async_trait;
use codec::{Decode, Encode};
use futures::{
	channel::oneshot,
	future::BoxFuture,
	stream::{FuturesUnordered, StreamExt},
};
use futures_timer::Delay;
use sc_authority_discovery::Service;
use sc_client_api::{BlockchainEvents, HeaderBackend};
use sc_network::{
	multiaddr::Protocol, service::traits::NetworkService, IfDisconnected, Multiaddr,
	NetworkRequest, NetworkStateInfo, ProtocolName,
};
use sc_network_types::PeerId;
use sp_authority_discovery::AuthorityId;
use sp_consensus::BlockOrigin;
use sp_runtime::traits::{Block as BlockT, BlockNumber, Header as HeaderT};
use std::{
	collections::{HashSet, VecDeque},
	marker::PhantomData,
	sync::Arc,
	time::Duration,
};
use tracing::{debug, trace, warn};

/// Log target for this module.
const LOG_TARGET: &str = "jam-package-sync::fetcher";

/// Blocks more than this many behind the current best are not worth fetching.
pub const MAX_BEHIND_BEST: u32 = 16;

/// Maximum number of retries of one block's package info.
pub const MAX_ATTEMPTS: u32 = 6;

/// Delay between two retries of the same block.
pub const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Maximum number of `fetch_one` jobs running at the same time.
pub const MAX_IN_FLIGHT: usize = 8;

/// The collators to ask for a block's package info, author first, self excluded.
pub trait PeerTargets<Block: BlockT> {
	/// The ordered list of authorities to try for `header`.
	fn targets(&self, header: &Block::Header) -> Vec<AuthorityId>;
}

/// Why a [`PackageInfoAcceptor`] refused a fetched package info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptError {
	/// This response is unusable, but another peer may still answer correctly: try the next peer.
	Rejected(String),
	/// This block cannot be used at all: drop the fetch job.
	Unusable(String),
}

/// Rebuilds and verifies a work package from a fetched [`PackageInfo`].
///
/// Implemented in `polkadot-omni-node-lib` where the JAM types and the local client are available.
#[async_trait]
pub trait PackageInfoAcceptor<Block: BlockT>: Send + Sync + 'static {
	/// Whether this block is already known, so it does not have to be fetched.
	fn is_known(&self, block: &Block::Hash) -> bool;

	/// Rebuild and verify the work package of `header` from `info`, fetched from `from`.
	async fn accept(
		&self,
		header: &Block::Header,
		info: PackageInfo,
		from: PeerId,
	) -> Result<(), AcceptError>;
}

/// Everything [`run_package_info_fetcher`] needs.
pub struct PackageFetcherParams<Block, Client, Targets, Acceptor> {
	/// The parachain network.
	pub network: Arc<dyn NetworkService>,
	/// The authority-discovery service, used to resolve authorities to peer ids.
	pub authority_discovery: Service,
	/// The package-info protocol name.
	pub info_protocol: ProtocolName,
	/// The parachain client, for its import notification stream.
	pub client: Arc<Client>,
	/// The collators to ask, author first.
	pub targets: Arc<Targets>,
	/// The acceptor that rebuilds and verifies the work package.
	pub acceptor: Arc<Acceptor>,
	/// `Block` is only used in the type signatures.
	pub _marker: PhantomData<Block>,
}

/// Whether a newly imported block is worth fetching package info for.
///
/// Skips blocks this node authored, blocks it already knows, and blocks more than
/// [`MAX_BEHIND_BEST`] behind the best block.
pub fn should_fetch<N: BlockNumber>(
	origin: &BlockOrigin,
	number: N,
	best_number: N,
	known: bool,
) -> bool {
	if matches!(origin, BlockOrigin::Own) || known {
		return false;
	}

	best_number.saturating_sub(number) <= MAX_BEHIND_BEST.into()
}

/// The delay before retrying a block whose fetch failed, or `None` when the retry budget is
/// exhausted.
pub fn retry_delay(attempt: u32) -> Option<Duration> {
	if attempt < MAX_ATTEMPTS {
		Some(RETRY_DELAY)
	} else {
		None
	}
}

/// Extract the peer ids from a set of authority-discovery addresses, dropping addresses without a
/// `p2p` component and our own peer id.
pub fn peer_ids(addrs: HashSet<Multiaddr>, local: &PeerId) -> Vec<PeerId> {
	addrs
		.into_iter()
		.filter_map(|addr| {
			addr.iter().find_map(|protocol| match protocol {
				Protocol::P2p(multihash) => PeerId::from_multihash(multihash).ok(),
				_ => None,
			})
		})
		.filter(|peer| peer != local)
		.collect()
}

/// The result of one [`fetch_one`] job.
enum FetchResult {
	/// A peer accepted the package info.
	Done,
	/// The block cannot be used at all; do not retry.
	Unusable,
	/// No peer could answer; retry if the budget allows.
	Retry,
}

/// A job waiting for a free in-flight slot.
struct Pending<Block: BlockT> {
	hash: Block::Hash,
	header: Block::Header,
	attempt: u32,
}

/// A retry whose delay has elapsed.
struct RetryDue<Block: BlockT> {
	hash: Block::Hash,
	header: Block::Header,
	attempt: u32,
}

/// The completion of one in-flight job.
enum FetchCompletion<Block: BlockT> {
	/// The package info was accepted.
	Done { hash: Block::Hash },
	/// The block is unusable.
	Unusable { hash: Block::Hash },
	/// No peer could answer; `attempt` is the number of retries already performed.
	Retry { hash: Block::Hash, header: Block::Header, attempt: u32 },
}

/// Start one `fetch_one` job for `hash`, returning a boxed future the event loop can hold.
fn start_fetch<Block, Targets, Acceptor>(
	network: Arc<dyn NetworkService>,
	authority_discovery: Service,
	protocol: ProtocolName,
	targets: Arc<Targets>,
	acceptor: Arc<Acceptor>,
	header: Block::Header,
	hash: Block::Hash,
	attempt: u32,
) -> BoxFuture<'static, FetchCompletion<Block>>
where
	Block: BlockT,
	Targets: PeerTargets<Block> + Send + Sync + 'static,
	Acceptor: PackageInfoAcceptor<Block>,
{
	Box::pin(async move {
		let result = fetch_one::<Block, Targets, Acceptor>(
			network,
			authority_discovery,
			protocol,
			targets,
			acceptor,
			&header,
			&hash,
		)
		.await;

		match result {
			FetchResult::Done => FetchCompletion::Done { hash },
			FetchResult::Unusable => FetchCompletion::Unusable { hash },
			FetchResult::Retry => FetchCompletion::Retry { hash, header, attempt },
		}
	})
}

/// Ask the target collators, one peer at a time, for the package info of `hash`.
async fn fetch_one<Block, Targets, Acceptor>(
	network: Arc<dyn NetworkService>,
	mut authority_discovery: Service,
	protocol: ProtocolName,
	targets: Arc<Targets>,
	acceptor: Arc<Acceptor>,
	header: &Block::Header,
	hash: &Block::Hash,
) -> FetchResult
where
	Block: BlockT,
	Targets: PeerTargets<Block> + Send + Sync + 'static,
	Acceptor: PackageInfoAcceptor<Block>,
{
	let request = PackageInfoRequest { block_hash: *hash }.encode();

	for authority in targets.targets(header) {
		let Some(addrs) = authority_discovery.get_addresses_by_authority_id(authority).await else {
			trace!(target: LOG_TARGET, "No address known for a target authority");
			continue;
		};

		for peer in peer_ids(addrs, &network.local_peer_id()) {
			let (tx, rx) = oneshot::channel();
			network.start_request(
				peer,
				protocol.clone(),
				request.clone(),
				None,
				tx,
				IfDisconnected::TryConnect,
			);

			let payload = match rx.await {
				Ok(Ok((payload, _))) => payload,
				Ok(Err(e)) => {
					debug!(target: LOG_TARGET, "Package info request to {peer} failed: {e}");
					continue;
				},
				Err(_) => continue,
			};

			match PackageInfoResponse::decode(&mut payload.as_slice()) {
				Ok(PackageInfoResponse::Known(info)) => {
					match acceptor.accept(header, info, peer).await {
						Ok(()) => return FetchResult::Done,
						Err(AcceptError::Rejected(reason)) => {
							warn!(target: LOG_TARGET, "Rejected package info from {peer}: {reason}");
							continue;
						},
						Err(AcceptError::Unusable(reason)) => {
							warn!(target: LOG_TARGET, "Unusable package info from {peer}: {reason}");
							return FetchResult::Unusable;
						},
					}
				},
				Ok(PackageInfoResponse::Unknown) => continue,
				Err(e) => {
					debug!(target: LOG_TARGET, "Failed to decode package info from {peer}: {e}");
					continue;
				},
			}
		}
	}

	FetchResult::Retry
}

/// Run the package-info fetcher until the import notification stream ends.
pub async fn run_package_info_fetcher<Block, Client, Targets, Acceptor>(
	params: PackageFetcherParams<Block, Client, Targets, Acceptor>,
) where
	Block: BlockT,
	Client: HeaderBackend<Block> + BlockchainEvents<Block>,
	Targets: PeerTargets<Block> + Send + Sync + 'static,
	Acceptor: PackageInfoAcceptor<Block>,
{
	let PackageFetcherParams {
		network,
		authority_discovery,
		info_protocol,
		client,
		targets,
		acceptor,
		..
	} = params;

	let mut notifications = client.import_notification_stream().fuse();
	let mut in_flight: FuturesUnordered<BoxFuture<'static, FetchCompletion<Block>>> =
		FuturesUnordered::new();
	let mut retry_delays: FuturesUnordered<BoxFuture<'static, RetryDue<Block>>> =
		FuturesUnordered::new();
	let mut active: HashSet<Block::Hash> = HashSet::new();
	let mut pending: VecDeque<Pending<Block>> = VecDeque::new();

	loop {
		futures::select! {
			notification = notifications.next() => {
				let Some(notification) = notification else { break };

				let best_number = client.info().best_number;
				let known = acceptor.is_known(&notification.hash);
				if !should_fetch(
					&notification.origin,
					*notification.header.number(),
					best_number,
					known,
				) {
					continue;
				}
				if active.contains(&notification.hash) {
					continue;
				}
				active.insert(notification.hash);

				let job = Pending {
					hash: notification.hash,
					header: notification.header,
					attempt: 0,
				};

				if in_flight.len() < MAX_IN_FLIGHT {
					in_flight.push(start_fetch::<Block, Targets, Acceptor>(
						network.clone(),
						authority_discovery.clone(),
						info_protocol.clone(),
						targets.clone(),
						acceptor.clone(),
						job.header,
						job.hash,
						job.attempt,
					));
				} else {
					pending.push_back(job);
				}
			}
			completion = in_flight.select_next_some() => {
				match completion {
					FetchCompletion::Done { hash } | FetchCompletion::Unusable { hash } => {
						active.remove(&hash);
					},
					FetchCompletion::Retry { hash, header, attempt } => {
						if let Some(delay) = retry_delay(attempt) {
							retry_delays.push(Box::pin(async move {
								Delay::new(delay).await;
								RetryDue { hash, header, attempt: attempt + 1 }
							}));
						} else {
							warn!(
								target: LOG_TARGET,
								"Giving up on package info for {hash:?} after {} attempts",
								attempt + 1,
							);
							active.remove(&hash);
						}
					},
				}

				while in_flight.len() < MAX_IN_FLIGHT {
					let Some(job) = pending.pop_front() else { break };
					active.insert(job.hash);
					in_flight.push(start_fetch::<Block, Targets, Acceptor>(
						network.clone(),
						authority_discovery.clone(),
						info_protocol.clone(),
						targets.clone(),
						acceptor.clone(),
						job.header,
						job.hash,
						job.attempt,
					));
				}
			}
			retry = retry_delays.select_next_some() => {
				let RetryDue { hash, header, attempt } = retry;

				if in_flight.len() < MAX_IN_FLIGHT {
					in_flight.push(start_fetch::<Block, Targets, Acceptor>(
						network.clone(),
						authority_discovery.clone(),
						info_protocol.clone(),
						targets.clone(),
						acceptor.clone(),
						header,
						hash,
						attempt,
					));
				} else {
					pending.push_back(Pending { hash, header, attempt });
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_fetch_skips_own_known_and_stale() {
		assert!(!should_fetch(&BlockOrigin::Own, 5u32, 5u32, false));
		assert!(!should_fetch(&BlockOrigin::NetworkBroadcast, 5u32, 5u32, true));
		assert!(should_fetch(&BlockOrigin::NetworkBroadcast, 5u32, 10u32, false));
		assert!(should_fetch(&BlockOrigin::NetworkBroadcast, 0u32, MAX_BEHIND_BEST, false));
		assert!(!should_fetch(&BlockOrigin::NetworkBroadcast, 0u32, MAX_BEHIND_BEST + 1, false));
		assert!(!should_fetch(&BlockOrigin::Genesis, 0u32, 100u32, false));
	}

	#[test]
	fn retry_delay_stops_after_max_attempts() {
		assert_eq!(retry_delay(0), Some(RETRY_DELAY));
		assert_eq!(retry_delay(MAX_ATTEMPTS - 1), Some(RETRY_DELAY));
		assert_eq!(retry_delay(MAX_ATTEMPTS), None);
		assert_eq!(retry_delay(MAX_ATTEMPTS + 10), None);
	}

	#[test]
	fn peer_ids_extracts_and_excludes_local() {
		let local = PeerId::random();
		let first = PeerId::random();
		let second = PeerId::random();

		let addrs: HashSet<Multiaddr> = [
			Multiaddr::empty().with(Protocol::P2p(first.into())),
			Multiaddr::empty().with(Protocol::P2p(second.into())),
			Multiaddr::empty().with(Protocol::P2p(local.into())),
			Multiaddr::empty().with(Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST)),
		]
		.into_iter()
		.collect();

		let mut peers = peer_ids(addrs, &local);
		peers.sort();

		let mut expected = vec![first, second];
		expected.sort();

		assert_eq!(peers, expected);
	}
}
