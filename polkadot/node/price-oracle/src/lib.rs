
use std::{collections::HashMap, sync::Arc, time::Duration};

use codec::{Decode, Encode};
use log::{debug, info, warn};
use parking_lot::RwLock;
use sc_network::{
	config::{NonReservedPeerMode, SetConfig},
	service::{
		traits::{NotificationEvent, NotificationService, ValidationResult},
		NotificationMetrics,
	},
	types::ProtocolName,
	NetworkBackend,
};
use sc_network_types::PeerId;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_babe::{AuthorityIndex, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_keystore::KeystorePtr;
use sp_price_oracle::{Nudge, PairId, PriceOracleApi, PriceOracleInherentData, SignedNudge};
use sp_runtime::{
	traits::{Block as BlockT, Saturating, Zero},
	FixedU128,
};

const LOG_TARGET: &str = "price-oracle";
/// A single `(PairId, SignedNudge)` message is ~102 bytes; 16KB allows batching up to ~160.
const MAX_NOTIFICATION_SIZE: u64 = 16 * 1024;
/// Match BABE slot time — one price fetch per slot.
const PRICE_FETCH_INTERVAL: Duration = Duration::from_secs(6);
/// Match BABE slot time — one nudge broadcast per slot.
const GOSSIP_INTERVAL: Duration = Duration::from_secs(6);
/// Prune every 2 slots to avoid churn while still cleaning up promptly.
const PRUNE_INTERVAL: Duration = Duration::from_secs(12);

mod fetcher;

pub use fetcher::PriceFetcher;

// Needs to be discussed
fn pick_random_index(len: usize) -> usize {
	let seed = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default()
		.subsec_nanos();
	seed as usize % len
}

/// Per-pair state inside the store.
#[derive(Default)]
struct PairState {
	/// Latest nudge per authority for this pair.
	nudges: HashMap<AuthorityIndex, SignedNudge>,
	/// Most-recent HTTP-derived price for this pair.
	cached_price: Option<FixedU128>,
}

/// Thread-safe per-pair nudge cache.
#[derive(Clone, Default)]
pub struct NudgeStore {
	inner: Arc<RwLock<HashMap<PairId, PairState>>>,
}

impl NudgeStore {
	pub fn new() -> Self {
		Self::default()
	}

	/// Insert a nudge for a pair, latest-wins per authority.
	pub fn insert(&self, pair_id: PairId, nudge: SignedNudge) {
		let mut inner = self.inner.write();
		let state = inner.entry(pair_id).or_default();
		match state.nudges.entry(nudge.authority_index) {
			std::collections::hash_map::Entry::Vacant(e) => {
				e.insert(nudge);
			},
			std::collections::hash_map::Entry::Occupied(mut e) => {
				if *nudge.slot >= *e.get().slot {
					e.insert(nudge);
				}
			},
		}
	}

	pub fn get_all_valid(
		&self,
		pair_id: PairId,
		current_slot: Slot,
		validity: u64,
	) -> Vec<SignedNudge> {
		let inner = self.inner.read();
		let Some(state) = inner.get(&pair_id) else { return Vec::new() };
		let current: u64 = (*current_slot).into();
		state
			.nudges
			.values()
			.filter(|n| {
				let nudge_slot: u64 = (*n.slot).into();
				current.saturating_sub(nudge_slot) < validity
			})
			.cloned()
			.collect()
	}

	pub fn prune(&self, pair_id: PairId, current_slot: Slot, validity: u64) {
		let mut inner = self.inner.write();
		let Some(state) = inner.get_mut(&pair_id) else { return };
		let current: u64 = (*current_slot).into();
		state.nudges.retain(|_, n| {
			let nudge_slot: u64 = (*n.slot).into();
			current.saturating_sub(nudge_slot) < validity
		});
	}

	pub fn set_cached_price(&self, pair_id: PairId, price: FixedU128) {
		let mut inner = self.inner.write();
		inner.entry(pair_id).or_default().cached_price = Some(price);
	}

	pub fn cached_price(&self, pair_id: PairId) -> Option<FixedU128> {
		let inner = self.inner.read();
		inner.get(&pair_id).and_then(|s| s.cached_price)
	}

	/// All pair ids we have local state for (either a cached price or received nudges).
	pub fn known_pairs(&self) -> Vec<PairId> {
		let inner = self.inner.read();
		inner.keys().copied().collect()
	}
}

pub struct OracleProtocolPrototype {
	protocol_name: ProtocolName,
	notification_service: Box<dyn NotificationService>,
}

impl OracleProtocolPrototype {
	pub fn new<
		Hash: AsRef<[u8]>,
		Block: BlockT,
		Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
	>(
		genesis_hash: Hash,
		fork_id: Option<&str>,
		metrics: NotificationMetrics,
		peer_store_handle: Arc<dyn sc_network::peer_store::PeerStoreProvider>,
	) -> (Self, Net::NotificationProtocolConfig) {
		let genesis_hex = array_bytes::bytes2hex("", genesis_hash.as_ref());
		// Protocol version bump to /2: v1 single-pair and v2 multi-pair nodes do not interop.
		let protocol_name = if let Some(fork_id) = fork_id {
			format!("/{}/{}/price-oracle/2", genesis_hex, fork_id)
		} else {
			format!("/{}/price-oracle/2", genesis_hex)
		};

		let (config, notification_service) = Net::notification_config(
			protocol_name.clone().into(),
			Vec::new(),
			MAX_NOTIFICATION_SIZE,
			None,
			SetConfig {
				in_peers: 25,
				out_peers: 25,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Accept,
			},
			metrics,
			peer_store_handle,
		);

		(Self { protocol_name: protocol_name.into(), notification_service }, config)
	}
}

/// Gossip envelope — what the wire carries for every nudge notification.
#[derive(Encode, Decode)]
struct GossipMessage {
	pair_id: PairId,
	nudge: SignedNudge,
}

/// Run the price oracle gossip service.
pub async fn run<Block, Client>(
	prototype: OracleProtocolPrototype,
	client: Arc<Client>,
	keystore: KeystorePtr,
	nudge_store: NudgeStore,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block> + 'static,
	Client::Api: sp_price_oracle::PriceOracleApi<Block>,
{
	let protocol_name = prototype.protocol_name;
	let mut notification_service = prototype.notification_service;
	let fetcher = PriceFetcher::new();
	let mut peers = std::collections::HashSet::<PeerId>::new();

	let mut price_fetch_timer = tokio::time::interval(PRICE_FETCH_INTERVAL);
	let mut gossip_timer = tokio::time::interval(GOSSIP_INTERVAL);
	let mut prune_timer = tokio::time::interval(PRUNE_INTERVAL);

	info!(target: LOG_TARGET, "🔮 Price oracle service started on protocol {}", protocol_name);

	loop {
		tokio::select! {
			_ = price_fetch_timer.tick() => {
				let best_hash = client.info().best_hash;

				let all_endpoints = match client.runtime_api().endpoint_list(best_hash) {
					Ok(v) => v,
					Err(e) => {
						warn!(target: LOG_TARGET, "Failed to get endpoint lists: {}", e);
						continue;
					},
				};

				if all_endpoints.is_empty() {
					debug!(target: LOG_TARGET, "No pairs / endpoints configured in runtime");
					continue;
				}

				// For each pair, try endpoints in randomised order until one works.
				let mut batched: Vec<(PairId, Vec<(u8, Vec<u8>)>)> = Vec::new();
				for (pair_id, endpoints) in all_endpoints {
					if endpoints.is_empty() {
						continue;
					}
					let urls: Vec<(u8, String)> = endpoints
						.into_iter()
						.filter_map(|(id, url_bytes)| {
							String::from_utf8(url_bytes).ok().map(|url| (id, url))
						})
						.collect();
					if urls.is_empty() {
						continue;
					}

					let primary_idx = pick_random_index(urls.len());
					let mut order: Vec<usize> = (0..urls.len()).collect();
					order.swap(0, primary_idx);

					for idx in order {
						let (id, ref url) = urls[idx];
						match fetcher.fetch_raw(url).await {
							Ok(bytes) => {
								batched.push((pair_id, vec![(id, bytes)]));
								break;
							},
							Err(e) => {
								warn!(
									target: LOG_TARGET,
									"Pair {} endpoint {} ({}) failed: {}, trying fallback",
									pair_id, id, url, e,
								);
							},
						}
					}
				}

				if batched.is_empty() {
					warn!(target: LOG_TARGET, "All endpoints failed across all pairs");
					continue;
				}

				match client.runtime_api().decode_results(best_hash, batched) {
					Ok(decoded) => {
						for (pair_id, inner) in decoded {
							for maybe_price in inner {
								if let Some(price) = maybe_price {
									debug!(
										target: LOG_TARGET,
										"Pair {} decoded price: {}",
										pair_id, price,
									);
									nudge_store.set_cached_price(pair_id, price);
									break;
								} else {
									warn!(
										target: LOG_TARGET,
										"Pair {} failed to decode response",
										pair_id,
									);
								}
							}
						}
					},
					Err(e) => warn!(target: LOG_TARGET, "decode_results runtime call failed: {}", e),
				}
			},

			_ = gossip_timer.tick() => {
				let best_hash = client.info().best_hash;
				let pair_ids = match client.runtime_api().list_pairs(best_hash) {
					Ok(v) => v,
					Err(e) => {
						warn!(target: LOG_TARGET, "list_pairs failed: {}", e);
						continue;
					},
				};

				for pair_id in pair_ids {
					if let Some(nudge_msg) = produce_and_sign_nudge::<Block, Client>(
						&client,
						&keystore,
						&nudge_store,
						pair_id,
					) {
						let envelope = GossipMessage { pair_id, nudge: nudge_msg.clone() };
						let encoded = envelope.encode();
						nudge_store.insert(pair_id, nudge_msg);

						for peer in &peers {
							notification_service.send_sync_notification(peer, encoded.clone());
						}

						debug!(
							target: LOG_TARGET,
							"Pair {}: gossipped nudge to {} peers",
							pair_id, peers.len(),
						);
					}
				}
			},

			_ = prune_timer.tick() => {
				let best_hash = client.info().best_hash;
				let pair_ids = match client.runtime_api().list_pairs(best_hash) {
					Ok(v) => v,
					Err(_) => continue,
				};
				let slot = get_current_slot::<Block, Client>(&client);
				for pair_id in pair_ids {
					if let Ok(Some(cfg)) = client.runtime_api().pair_config(best_hash, pair_id) {
						nudge_store.prune(pair_id, slot, cfg.nudge_validity);
					}
				}
				debug!(target: LOG_TARGET, "Pruned stale nudges");
			},

			event = notification_service.next_event() => {
				match event {
					Some(NotificationEvent::NotificationReceived { peer, notification }) => {
						match GossipMessage::decode(&mut notification.as_ref()) {
							Ok(GossipMessage { pair_id, nudge }) => {
								debug!(
									target: LOG_TARGET,
									"Received nudge from peer {:?} for pair {}: {:?}",
									peer, pair_id, nudge.nudge,
								);
								nudge_store.insert(pair_id, nudge);
							},
							Err(e) => {
								debug!(
									target: LOG_TARGET,
									"Failed to decode gossip envelope from {:?}: {}",
									peer, e,
								);
							},
						}
					},
					Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) => {
						let _ = result_tx.send(ValidationResult::Accept);
					},
					Some(NotificationEvent::NotificationStreamOpened { peer, .. }) => {
						peers.insert(peer);
						debug!(target: LOG_TARGET, "Peer connected: {:?}, total: {}", peer, peers.len());
					},
					Some(NotificationEvent::NotificationStreamClosed { peer }) => {
						peers.remove(&peer);
						debug!(target: LOG_TARGET, "Peer disconnected: {:?}, total: {}", peer, peers.len());
					},
					None => {
						warn!(target: LOG_TARGET, "Notification stream ended");
						return;
					},
				}
			},
		}
	}
}

fn get_current_slot<Block, Client>(client: &Arc<Client>) -> Slot
where
	Block: BlockT,
	Client: HeaderBackend<Block>,
{
	use sp_consensus_babe::digests::CompatibleDigestItem;
	use sp_runtime::traits::Header as _;

	let best_hash = client.info().best_hash;
	client
		.header(best_hash)
		.ok()
		.flatten()
		.and_then(|header| header.digest().logs().iter().find_map(|log| log.as_babe_pre_digest()))
		.map(|pre_digest| pre_digest.slot())
		.unwrap_or(Slot::from(0u64))
}

fn produce_and_sign_nudge<Block, Client>(
	client: &Arc<Client>,
	keystore: &KeystorePtr,
	nudge_store: &NudgeStore,
	pair_id: PairId,
) -> Option<SignedNudge>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: sp_price_oracle::PriceOracleApi<Block>,
{
	let best_hash = client.info().best_hash;

	let onchain_price = client.runtime_api().current_price(best_hash, pair_id).ok()?;
	let authorities = client.runtime_api().authorities(best_hash).ok()?;

	let cached_price = nudge_store.cached_price(pair_id)?;

	let nudge = if cached_price >= onchain_price { Nudge::Up } else { Nudge::Down };

	let slot = get_current_slot::<Block, Client>(client);

	let local_keys = keystore.sr25519_public_keys(sp_consensus_babe::KEY_TYPE);
	let (authority_index, local_public) =
		authorities.iter().enumerate().find_map(|(i, auth)| {
			let public = sp_core::sr25519::Public::from(auth.clone());
			local_keys.contains(&public).then_some((i as u32, public))
		})?;

	let raw_sig = keystore
		.sr25519_sign(
			sp_consensus_babe::KEY_TYPE,
			&local_public,
			&SignedNudge::signing_payload(&nudge, slot),
		)
		.ok()
		.flatten()?;

	Some(SignedNudge { nudge, slot, authority_index, signature: AuthoritySignature::from(raw_sig) })
}

/// Build the per-pair inherent data for the block at `parent_hash`.
///
/// For each registered pair, selects up to `needed = diff / epsilon` nudges in the appropriate
/// direction from the store. Pairs without a cached price still get an empty entry so that a
/// `inherent_mandatory=true` pair with `min_nudges=0` is still satisfied.
pub fn create_inherent_data<Block, Client>(
	client: &Arc<Client>,
	nudge_store: &NudgeStore,
	parent_hash: Block::Hash,
) -> PriceOracleInherentData
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: sp_price_oracle::PriceOracleApi<Block>,
{
	let pair_ids = match client.runtime_api().list_pairs(parent_hash) {
		Ok(v) => v,
		Err(e) => {
			warn!(target: LOG_TARGET, "Failed to list pairs: {}", e);
			return Vec::new();
		},
	};

	let slot = get_current_slot::<Block, Client>(client);
	let mut out: PriceOracleInherentData = Vec::with_capacity(pair_ids.len());

	for pair_id in pair_ids {
		let cfg = match client.runtime_api().pair_config(parent_hash, pair_id) {
			Ok(Some(cfg)) => cfg,
			Ok(None) => continue,
			Err(e) => {
				warn!(target: LOG_TARGET, "pair_config({}) failed: {}", pair_id, e);
				continue;
			},
		};
		let onchain_price = client
			.runtime_api()
			.current_price(parent_hash, pair_id)
			.unwrap_or_else(|_| FixedU128::zero());

		let Some(cached_price) = nudge_store.cached_price(pair_id) else {
			out.push((pair_id, Vec::new()));
			continue;
		};

		let all_nudges = nudge_store.get_all_valid(pair_id, slot, cfg.nudge_validity);

		if cfg.epsilon.is_zero() || all_nudges.is_empty() {
			out.push((pair_id, Vec::new()));
			continue;
		}

		let diff = if cached_price >= onchain_price {
			cached_price.saturating_sub(onchain_price)
		} else {
			onchain_price.saturating_sub(cached_price)
		};
		let direction = if cached_price >= onchain_price { Nudge::Up } else { Nudge::Down };
		let needed: usize = (diff.into_inner() / cfg.epsilon.into_inner()) as usize;

		let mut selected = Vec::new();
		for n in &all_nudges {
			if selected.len() >= needed {
				break;
			}
			if n.nudge == direction {
				selected.push(n.clone());
			}
		}

		info!(
			target: LOG_TARGET,
			"Pair {}: onchain={:?}, cached={:?}, direction={:?}, needed={}, selected={}",
			pair_id, onchain_price, cached_price, direction, needed, selected.len(),
		);

		out.push((pair_id, selected));
	}

	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_babe::AuthoritySignature;
	use sp_consensus_slots::Slot;
	use sp_core::{crypto::Pair as PairT, sr25519};
	use sp_price_oracle::{Nudge, SignedNudge};

	fn make_nudge(slot: u64, authority_index: u32, nudge: Nudge) -> SignedNudge {
		let pair = sr25519::Pair::from_seed(&[authority_index as u8; 32]);
		let slot = Slot::from(slot);
		let payload = SignedNudge::signing_payload(&nudge, slot);
		let sig = pair.sign(&payload);
		SignedNudge { nudge, slot, authority_index, signature: AuthoritySignature::from(sig) }
	}

	#[test]
	fn nudge_store_insert_and_retrieve() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(10, 0, Nudge::Up));
		store.insert(0, make_nudge(10, 1, Nudge::Down));
		store.insert(0, make_nudge(11, 2, Nudge::Up));

		let all = store.get_all_valid(0, Slot::from(11u64), 5);
		assert_eq!(all.len(), 3);
	}

	#[test]
	fn nudge_store_keeps_latest_per_authority() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(10, 0, Nudge::Up));
		store.insert(0, make_nudge(11, 0, Nudge::Down));

		let all = store.get_all_valid(0, Slot::from(11u64), 5);
		assert_eq!(all.len(), 1);
		assert_eq!(all[0].nudge, Nudge::Down);
		assert_eq!(*all[0].slot, 11);
	}

	#[test]
	fn nudge_store_rejects_older_from_same_authority() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(11, 0, Nudge::Up));
		store.insert(0, make_nudge(10, 0, Nudge::Down));

		let all = store.get_all_valid(0, Slot::from(11u64), 5);
		assert_eq!(all.len(), 1);
		assert_eq!(all[0].nudge, Nudge::Up);
		assert_eq!(*all[0].slot, 11);
	}

	#[test]
	fn get_all_valid_filters_stale() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(5, 0, Nudge::Up));
		store.insert(0, make_nudge(10, 1, Nudge::Up));
		store.insert(0, make_nudge(15, 2, Nudge::Up));

		let valid = store.get_all_valid(0, Slot::from(16u64), 5);
		assert_eq!(valid.len(), 1);
		assert_eq!(valid[0].authority_index, 2);
	}

	#[test]
	fn prune_removes_stale_nudges() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(1, 0, Nudge::Up));
		store.insert(0, make_nudge(5, 1, Nudge::Down));
		store.insert(0, make_nudge(10, 2, Nudge::Up));

		store.prune(0, Slot::from(12u64), 5);

		let remaining = store.get_all_valid(0, Slot::from(12u64), 100);
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].authority_index, 2);
	}

	#[test]
	fn cached_price_round_trips() {
		let store = NudgeStore::new();
		assert!(store.cached_price(0).is_none());
		store.set_cached_price(0, FixedU128::from_u32(5));
		assert_eq!(store.cached_price(0), Some(FixedU128::from_u32(5)));
	}

	#[test]
	fn pairs_are_isolated() {
		let store = NudgeStore::new();
		store.insert(0, make_nudge(10, 0, Nudge::Up));
		store.insert(1, make_nudge(10, 0, Nudge::Down));

		let a = store.get_all_valid(0, Slot::from(10u64), 5);
		let b = store.get_all_valid(1, Slot::from(10u64), 5);
		assert_eq!(a.len(), 1);
		assert_eq!(a[0].nudge, Nudge::Up);
		assert_eq!(b.len(), 1);
		assert_eq!(b[0].nudge, Nudge::Down);

		store.set_cached_price(0, FixedU128::from_u32(1));
		store.set_cached_price(1, FixedU128::from_u32(2));
		assert_eq!(store.cached_price(0), Some(FixedU128::from_u32(1)));
		assert_eq!(store.cached_price(1), Some(FixedU128::from_u32(2)));
	}
}
