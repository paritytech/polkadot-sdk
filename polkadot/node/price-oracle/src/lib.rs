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
const MAX_NOTIFICATION_SIZE: u64 = 16 * 1024;
const PRICE_FETCH_INTERVAL: Duration = Duration::from_secs(6);
const GOSSIP_INTERVAL: Duration = Duration::from_secs(6);
const PRUNE_INTERVAL: Duration = Duration::from_secs(12);

mod fetcher;

pub use fetcher::PriceFetcher;

// ------ Shared nudge store ------

/// Thread-safe store of collected nudges, keyed by (authority_index, slot).
#[derive(Clone)]
pub struct NudgeStore {
	inner: Arc<RwLock<NudgeStoreInner>>,
}

struct NudgeStoreInner {
	nudges: HashMap<(AuthorityIndex, Slot), SignedNudge>,
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
		let key = (nudge.authority_index, nudge.slot);
		self.inner.write().nudges.insert(key, nudge);
	}

	pub fn get_all_valid(&self, current_slot: Slot, validity: u64) -> Vec<SignedNudge> {
		let inner = self.inner.read();
		let current: u64 = (*current_slot).into();
		inner
			.nudges
			.values()
			.filter(|n| {
				let slot_val: u64 = (*n.slot).into();
				current.saturating_sub(slot_val) < validity
			})
			.cloned()
			.collect()
	}

	pub fn prune(&self, current_slot: Slot, validity: u64) {
		let mut inner = self.inner.write();
		let current: u64 = (*current_slot).into();
		inner.nudges.retain(|_, n| {
			let slot_val: u64 = (*n.slot).into();
			current.saturating_sub(slot_val) < validity
		});
	}

	pub fn set_cached_price(&self, price: FixedU128) {
		self.inner.write().cached_price = Some(price);
	}

	pub fn cached_price(&self) -> Option<FixedU128> {
		self.inner.read().cached_price
	}
}

// ------ Protocol setup ------

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

// ------ Oracle service ------

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

	// Find our authority index and sign
	for (index, authority) in authorities.iter().enumerate() {
		let public = sp_core::sr25519::Public::from(authority.clone());
		if let Ok(Some(raw_sig)) = keystore.sr25519_sign(
			sp_consensus_babe::KEY_TYPE,
			&public,
			&SignedNudge::signing_payload(&nudge, slot),
		) {
			let signature = AuthoritySignature::from(raw_sig);
			return Some(SignedNudge { nudge, slot, authority_index: index as u32, signature });
		}
	}

	debug!(target: LOG_TARGET, "No BABE key found in keystore for signing nudge");
	None
}

// ------ Inherent data provider ------

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
		"Block author: onchain={}, cached={}, direction={:?}, needed={}, selected={}",
		onchain_price, cached_price, direction, needed, selected.len(),
	);

	selected
}
