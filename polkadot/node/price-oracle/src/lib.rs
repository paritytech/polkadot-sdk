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
use sp_price_oracle::{Nudge, PriceOracleApi, PriceOracleInherentData, SignedNudge};
use sp_runtime::{
	traits::{Block as BlockT, Saturating, Zero},
	FixedU128,
};

const LOG_TARGET: &str = "price-oracle";
/// A single SignedNudge is ~100 bytes; 16KB allows batching up to ~160 nudges in one message.
const MAX_NOTIFICATION_SIZE: u64 = 16 * 1024;
/// Match BABE slot time — one price fetch per slot.
const PRICE_FETCH_INTERVAL: Duration = Duration::from_secs(6);
/// Match BABE slot time — one nudge broadcast per slot.
const GOSSIP_INTERVAL: Duration = Duration::from_secs(6);
/// Prune every 2 slots to avoid churn while still cleaning up promptly.
const PRUNE_INTERVAL: Duration = Duration::from_secs(12);

mod fetcher;

pub use fetcher::PriceFetcher;

/// Thread-safe store of collected nudges, keyed by authority index.
/// Only the latest nudge per authority is kept.
#[derive(Clone)]
pub struct NudgeStore {
	inner: Arc<RwLock<NudgeStoreInner>>,
}

struct NudgeStoreInner {
	nudges: HashMap<AuthorityIndex, SignedNudge>,
	cached_price: Option<FixedU128>,
}

impl NudgeStore {
	pub fn new() -> Self {
		Self {
			inner: Arc::new(RwLock::new(NudgeStoreInner {
				nudges: HashMap::new(),
				cached_price: None,
			})),
		}
	}

	pub fn insert(&self, nudge: SignedNudge) {
		let mut inner = self.inner.write();
		match inner.nudges.entry(nudge.authority_index) {
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

	pub fn get_all_valid(&self, current_slot: Slot, validity: u64) -> Vec<SignedNudge> {
		let inner = self.inner.read();
		let current: u64 = (*current_slot).into();
		inner
			.nudges
			.values()
			.filter(|n| {
				let nudge_slot: u64 = (*n.slot).into();
				current.saturating_sub(nudge_slot) < validity
			})
			.cloned()
			.collect()
	}

	pub fn prune(&self, current_slot: Slot, validity: u64) {
		let mut inner = self.inner.write();
		let current: u64 = (*current_slot).into();
		inner.nudges.retain(|_, n| {
			let nudge_slot: u64 = (*n.slot).into();
			current.saturating_sub(nudge_slot) < validity
		});
	}

	pub fn set_cached_price(&self, price: FixedU128) {
		self.inner.write().cached_price = Some(price);
	}

	pub fn cached_price(&self) -> Option<FixedU128> {
		self.inner.read().cached_price
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
		let protocol_name = if let Some(fork_id) = fork_id {
			format!("/{}/{}/price-oracle/1", genesis_hex, fork_id)
		} else {
			format!("/{}/price-oracle/1", genesis_hex)
		};

		let (config, notification_service) = Net::notification_config(
			protocol_name.clone().into(),
			Vec::new(),
			MAX_NOTIFICATION_SIZE,
			None,
			SetConfig {
				// With ~300 validators, 25 in/out peers gives ~8% direct connectivity per node.
				// Matches the statement handler's defaults. Multi-hop gossip covers the rest.
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

/// Run the price oracle gossip service.
///
/// This spawns the background tasks:
/// 1. Periodically fetch the price from an external API and cache it
/// 2. Periodically produce and gossip a signed nudge
/// 3. Listen for incoming nudges from peers
/// 4. Periodically prune stale nudges
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
				match fetcher.fetch_dot_usd_price().await {
					Ok(price) => {
						debug!(target: LOG_TARGET, "Fetched DOT/USD price: {}", price);
						nudge_store.set_cached_price(price);
					},
					Err(e) => {
						warn!(target: LOG_TARGET, "Failed to fetch price: {}", e);
					},
				}
			},

			_ = gossip_timer.tick() => {
				if let Some(nudge_msg) = produce_and_sign_nudge::<Block, Client>(
					&client,
					&keystore,
					&nudge_store,
				) {
					let encoded = nudge_msg.encode();
					nudge_store.insert(nudge_msg);

					for peer in &peers {
						notification_service.send_sync_notification(peer, encoded.clone());
					}

					debug!(target: LOG_TARGET, "Gossipped nudge to {} peers", peers.len());
				}
			},

			_ = prune_timer.tick() => {
				let best_hash = client.info().best_hash;
				if let Ok(validity) = client.runtime_api().nudge_validity(best_hash) {
					let slot = get_current_slot::<Block, Client>(&client);
					nudge_store.prune(slot, validity);
					debug!(target: LOG_TARGET, "Pruned stale nudges");
				}
			},

			event = notification_service.next_event() => {
				match event {
					Some(NotificationEvent::NotificationReceived { peer, notification }) => {
						match SignedNudge::decode(&mut notification.as_ref()) {
							Ok(nudge) => {
								debug!(
									target: LOG_TARGET,
									"Received nudge from peer {:?}: {:?}",
									peer, nudge.nudge,
								);
								nudge_store.insert(nudge);
							},
							Err(e) => {
								debug!(
									target: LOG_TARGET,
									"Failed to decode nudge from {:?}: {}",
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

	// TODO: is this the slot of the current block being authored, or the parent block? 99% it is
	// the parent!
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
) -> Option<SignedNudge>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: sp_price_oracle::PriceOracleApi<Block>,
{
	let best_hash = client.info().best_hash;

	let onchain_price = client.runtime_api().current_price(best_hash).ok()?;
	let authorities = client.runtime_api().authorities(best_hash).ok()?;

	let cached_price = nudge_store.cached_price()?;

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

/// Creates the price oracle inherent data from the nudge store.
///
/// Called during block authoring to select a subset of valid nudges.
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
	let onchain_price = match client.runtime_api().current_price(parent_hash) {
		Ok(p) => p,
		Err(e) => {
			warn!(target: LOG_TARGET, "Failed to query onchain price: {}", e);
			return Vec::new();
		},
	};
	let epsilon = match client.runtime_api().epsilon(parent_hash) {
		Ok(e) => e,
		Err(e) => {
			warn!(target: LOG_TARGET, "Failed to query epsilon: {}", e);
			return Vec::new();
		},
	};
	let validity = match client.runtime_api().nudge_validity(parent_hash) {
		Ok(v) => v,
		Err(e) => {
			warn!(target: LOG_TARGET, "Failed to query nudge validity: {}", e);
			return Vec::new();
		},
	};

	let cached_price = match nudge_store.cached_price() {
		Some(p) => p,
		None => {
			debug!(target: LOG_TARGET, "No cached price available yet");
			return Vec::new();
		},
	};

	let slot = get_current_slot::<Block, Client>(client);
	let all_nudges = nudge_store.get_all_valid(slot, validity);

	if epsilon.is_zero() || all_nudges.is_empty() {
		return Vec::new();
	}

	// Determine direction and count, matching the honest validator sim logic:
	// neededBumps = min(round(abs(diff) / epsilon), available)
	let diff = if cached_price >= onchain_price {
		cached_price.saturating_sub(onchain_price)
	} else {
		onchain_price.saturating_sub(cached_price)
	};

	let direction = if cached_price >= onchain_price { Nudge::Up } else { Nudge::Down };

	let needed: u64 = if epsilon.is_zero() {
		0
	} else {
		// diff / epsilon: both are FixedU128, divide inner representations
		(diff.into_inner() / epsilon.into_inner()) as u64
	};

	let needed = needed as usize;

	// Select nudges in the desired direction, up to needed count
	let mut selected = Vec::new();
	for nudge in &all_nudges {
		if selected.len() >= needed {
			break;
		}
		if nudge.nudge == direction {
			selected.push(nudge.clone());
		}
	}

	info!(
		target: LOG_TARGET,
		"Block author: onchain={:?}, cached={:?}, direction={:?}, needed={}, selected={}",
		onchain_price, cached_price, direction, needed, selected.len(),
	);

	selected
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
		store.insert(make_nudge(10, 0, Nudge::Up));
		store.insert(make_nudge(10, 1, Nudge::Down));
		store.insert(make_nudge(11, 2, Nudge::Up));

		let all = store.get_all_valid(Slot::from(11u64), 5);
		assert_eq!(all.len(), 3);
	}

	#[test]
	fn nudge_store_keeps_latest_per_authority() {
		let store = NudgeStore::new();
		store.insert(make_nudge(10, 0, Nudge::Up));
		store.insert(make_nudge(11, 0, Nudge::Down));

		let all = store.get_all_valid(Slot::from(11u64), 5);
		assert_eq!(all.len(), 1);
		assert_eq!(all[0].nudge, Nudge::Down);
		assert_eq!(*all[0].slot, 11);
	}

	#[test]
	fn nudge_store_rejects_older_from_same_authority() {
		let store = NudgeStore::new();
		store.insert(make_nudge(11, 0, Nudge::Up));
		store.insert(make_nudge(10, 0, Nudge::Down));

		let all = store.get_all_valid(Slot::from(11u64), 5);
		assert_eq!(all.len(), 1);
		assert_eq!(all[0].nudge, Nudge::Up);
		assert_eq!(*all[0].slot, 11);
	}

	#[test]
	fn get_all_valid_filters_stale() {
		let store = NudgeStore::new();
		store.insert(make_nudge(5, 0, Nudge::Up));
		store.insert(make_nudge(10, 1, Nudge::Up));
		store.insert(make_nudge(15, 2, Nudge::Up));

		// validity=5, current_slot=16 → slot 5 is stale (16-5=11>=5), slot 10 is stale
		// (16-10=6>=5), slot 15 valid
		let valid = store.get_all_valid(Slot::from(16u64), 5);
		assert_eq!(valid.len(), 1);
		assert_eq!(valid[0].authority_index, 2);
	}

	#[test]
	fn prune_removes_stale_nudges() {
		let store = NudgeStore::new();
		store.insert(make_nudge(1, 0, Nudge::Up));
		store.insert(make_nudge(5, 1, Nudge::Down));
		store.insert(make_nudge(10, 2, Nudge::Up));

		store.prune(Slot::from(12u64), 5);

		// After pruning: slot 1 gone (12-1=11>=5), slot 5 gone (12-5=7>=5), slot 10 kept
		// (12-10=2<5)
		let remaining = store.get_all_valid(Slot::from(12u64), 100);
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].authority_index, 2);
	}

	#[test]
	fn cached_price_round_trips() {
		let store = NudgeStore::new();
		assert!(store.cached_price().is_none());

		store.set_cached_price(FixedU128::from_u32(5));
		assert_eq!(store.cached_price(), Some(FixedU128::from_u32(5)));
	}
}
