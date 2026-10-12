//! The DKG worker: watches session changes, produces the node's own dealing and
//! gossips it, and collects validated dealings from other validators.
//!
//! Flow per session (triggered by `Session::CurrentIndex` changes observed at
//! finalized blocks):
//! 1. Rebuild the DKG authority set from `Session::QueuedKeys` storage
//!    ([`crate::keys::queued_dkg_public_keys`]) and update the gossip filter.
//! 2. If the node's own BEEFY key is in the set and it has not dealt yet this
//!    session, produce a fresh a-DKG transcript ([`deal`]) and gossip it as a
//!    signed `DealingMessage`.
//! 3. Validated incoming dealings are collected for later aggregation (the
//!    validator→parachain submission leg is Phase 4).
//!
//! Dealing needs the dealer's secret scalar for the `ContributionReceipt` proof
//! of possession. The keystore cannot export secrets, so for now the worker
//! takes the dealing pair in-memory (dev/testnet only); a keystore-signable
//! receipt API in `adkg-vrf` is the production follow-up.

use std::{
	collections::BTreeSet,
	marker::PhantomData,
	sync::{atomic::Ordering, Arc},
};

use codec::{Decode, Encode};
use futures::{select, FutureExt, StreamExt};
use std::future::Future;
use log::{debug, error, info};
use rand::rngs::OsRng;

use sc_client_api::BlockchainEvents;
use sc_network::{NotificationService, ProtocolName};
use sc_network_gossip::{GossipEngine, Network as GossipNetwork, Syncing as GossipSyncing};
use sp_blockchain::HeaderBackend;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, OpaqueKeys};

use adkg_vrf::bls::vanilla::BlsSigner;
use adkg_vrf::dkg::{transcript::Transcript, Dkg};

use ark_bls12_381::Bls12_381;
use ark_serialize::CanonicalSerialize;

/// The block's hashing function (used for gossip topics and known-peers keys).
type HashingOf<B> = <<B as BlockT>::Header as HeaderT>::Hashing;

use crate::{
	communication::{
		gossip::{
			bls_crypto, dealing_sign_payload, dealings_topic, DealingMessage, GossipMessage,
			GossipValidator,
		},
		notification::Dealing,
		peers::KnownPeers,
	},
	keys::{self, AuthoritySetError},
	CollectionStatsHandle, LOG_TARGET,
};

/// DKG worker parameters.
pub struct WorkerParams<Client, Network, S, Bc, AccountId, Keys> {
	/// The blockchain client; used to read session state at finalized blocks.
	pub client: Arc<Client>,
	/// Keystore holding the node's BEEFY paired key (used to sign dealings).
	pub keystore: KeystorePtr,
	/// Dev/testnet only: the node's BEEFY paired keypair, used to produce the
	/// dealing's proof of possession (the keystore cannot export secrets).
	/// If `None`, the node validates/forwards dealings but does not deal.
	pub dealing_pair: Option<sp_core::ecdsa_bls381::Pair>,
	/// Network implementing gossip and peers reporting (e.g. `Arc<dyn NetworkService>`).
	pub network: Network,
	/// Syncing service implementing a sync oracle.
	pub sync: S,
	/// Handle for receiving notification events on the DKG gossip protocol.
	pub notification_service: Box<dyn NotificationService>,
	/// The DKG gossip protocol name, see
	/// [`crate::communication::dkg_protocol_name::gossip_protocol_name`].
	pub gossip_protocol_name: ProtocolName,
	/// Prometheus metrics registry, if any.
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
	pub _phantom: PhantomData<(Bc, AccountId, Keys)>,
}

/// Error produced by the DKG worker.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
	/// Failed to fetch the DKG authority set or session index.
	#[error(transparent)]
	AuthoritySet(#[from] AuthoritySetError),
	/// The node's own BEEFY key could not be converted to arkworks types.
	#[error("malformed own BEEFY key")]
	MalformedKey,
	/// The a-DKG dealing failed.
	#[error("a-DKG dealing failed: {0}")]
	Deal(String),
}

/// Extracts the a-DKG signer (secret scalar + G1/G2 public keys) from a BEEFY
/// paired keypair.
///
/// `Pair::to_raw_vec` for paired crypto is `ecdsa_secret (32B) || bls_secret
/// (32B)`; the BLS secret scalar is what the a-DKG dealer signs with. The
/// derived public keys are cross-checked against the paired public key so a
/// mismatched conversion fails loudly instead of producing a broken dealing.
pub(crate) fn signer_from_pair(
	pair: &sp_core::ecdsa_bls381::Pair,
) -> Result<(BlsSigner<Bls12_381>, bls_crypto::Public), WorkerError> {
	use ark_ec::{CurveGroup, PrimeGroup};
	use ark_ff::PrimeField;
	use sp_core::{crypto::ByteArray, Pair as _};

	let public = bls_crypto::Public::from_slice(pair.public().as_ref())
		.map_err(|_| WorkerError::MalformedKey)?;
	let pk_g2 = bls_crypto::to_ark_g2(&public).ok_or(WorkerError::MalformedKey)?;
	let pk_g1 = bls_crypto::to_ark_g1(&public).ok_or(WorkerError::MalformedKey)?;

	// w3f-bls `SecretKey::to_bytes` serializes the scalar little-endian.
	let raw = pair.to_raw_vec();
	let sk_bytes: [u8; 32] = raw[32..].try_into().map_err(|_| WorkerError::MalformedKey)?;
	let sk = ark_bls12_381::Fr::from_le_bytes_mod_order(&sk_bytes);

	// Sanity: the scalar must reproduce the public keys.
	if (ark_bls12_381::G1Projective::generator() * sk).into_affine() != pk_g1 ||
		(ark_bls12_381::G2Projective::generator() * sk).into_affine() != pk_g2
	{
		return Err(WorkerError::MalformedKey)
	}

	Ok((BlsSigner::<Bls12_381> { sk, bls_pk_g1: pk_g1, bls_pk_g2: pk_g2 }, public))
}

/// Produces the node's dealing (a-DKG transcript) for the given authority set.
///
/// BFT parameters: with `n` validators and `f = (n-1)/3`, the PVSS threshold is
/// `t = 2f+1` and any validator may deal (`t_dkg = f+1` dealers suffice).
pub(crate) fn deal(
	authority_set: &BTreeSet<bls_crypto::Public>,
	dealer_pair: &sp_core::ecdsa_bls381::Pair,
) -> Result<(Transcript<Bls12_381>, bls_crypto::Public), WorkerError> {
	let (signer, public) = signer_from_pair(dealer_pair)?;

	let n = authority_set.len();
	let f = n.saturating_sub(1) / 3;
	let signers_pks = authority_set.iter().filter_map(bls_crypto::to_ark_g2).collect::<Vec<_>>();
	let dealer_pks = authority_set.iter().filter_map(bls_crypto::to_ark_g1).collect::<Vec<_>>();

	let dkg = Dkg::<Bls12_381>::new(signers_pks, 2 * f + 1, dealer_pks, f + 1)
		.map_err(|e| WorkerError::Deal(format!("{e:?}")))?;
	dkg.deal_and_sign(&mut OsRng, (signer.sk, signer.bls_pk_g1))
		.map(|transcript| (transcript, public))
		.map_err(|e| WorkerError::Deal(format!("{e:?}")))
}

/// The DKG worker.
pub struct DkgWorker<B, Client, Network, S, Bc, AccountId, Keys>
where
	B: BlockT,
{
	client: Arc<Client>,
	keystore: KeystorePtr,
	dealing_pair: Option<sp_core::ecdsa_bls381::Pair>,
	gossip_engine: GossipEngine<B>,
	gossip_validator: Arc<GossipValidator<HashingOf<B>, Network>>,
	/// Session currently tracked by the gossip filter.
	current_session: Option<u32>,
	/// Session we have already dealt in (deal at most once per session).
	dealt_session: Option<u32>,
	/// Validated dealings received over gossip for the current session
	/// (to be aggregated/submitted on-chain — Phase 4).
	collected: Vec<(u32, Dealing<Vec<u8>>)>,
	/// Collection observability shared with the node that spawned the worker.
	collection_stats: CollectionStatsHandle,
	_phantom: PhantomData<(S, Bc, AccountId, Keys)>,
}

impl<B, Client, Network, S, Bc, AccountId, Keys> DkgWorker<B, Client, Network, S, Bc, AccountId, Keys>
where
	B: BlockT,
	Bc: sc_client_api::backend::Backend<B>,
	Client: HeaderBackend<B> + BlockchainEvents<B> + sc_client_api::StorageProvider<B, Bc> + 'static,
	Network: GossipNetwork<B> + sc_network::NetworkPeers + Clone + Send + Sync + 'static,
	S: GossipSyncing<B> + Clone + Send + Sync + 'static,
	AccountId: Decode + Ord,
	Keys: OpaqueKeys + Decode,
{
	/// Returns a new instance under the form of a `Future` that must be polled
	/// regularly to drive the DKG.
	pub fn new(worker_params: WorkerParams<Client, Network, S, Bc, AccountId, Keys>) -> Self {
		let WorkerParams {
			client,
			keystore,
			dealing_pair,
			network,
			sync,
			notification_service,
			gossip_protocol_name,
			prometheus_registry,
			_phantom: _,
		} = worker_params;

		let known_peers = Arc::new(parking_lot::Mutex::new(KnownPeers::<HashingOf<B>>::new()));
		let gossip_validator =
			Arc::new(GossipValidator::<HashingOf<B>, Network>::new(network.clone(), known_peers));
		let gossip_engine = GossipEngine::new(
			network,
			sync,
			notification_service,
			gossip_protocol_name,
			gossip_validator.clone(),
			prometheus_registry.as_ref(),
		);

		Self {
			client,
			keystore,
			dealing_pair,
			gossip_engine,
			gossip_validator,
			current_session: None,
			dealt_session: None,
			collected: Vec::new(),
			collection_stats: CollectionStatsHandle::default(),
			_phantom: PhantomData,
		}
	}

	/// A handle to this worker's collection stats (dealings received over
	/// gossip); clone before [`Self::run`] consumes the worker.
	pub fn collection_stats(&self) -> CollectionStatsHandle {
		self.collection_stats.clone()
	}

	/// Runs the worker: drives gossip and reacts to session changes.
	pub async fn run(mut self) {
		let topic = dealings_topic::<HashingOf<B>>();

		// Initialize the filter at the current finalized head.
		let finalized = self.client.info().finalized_hash;
		self.maybe_new_session(finalized);

		let mut finality_notifications = self.client.finality_notification_stream().fuse();
		let mut dealings = self.gossip_engine.messages_for(topic).fuse();

		loop {
			select! {
				notification = finality_notifications.next() => {
					let Some(notification) = notification else { return };
					self.maybe_new_session(notification.hash);
				},
				message = dealings.next() => {
					let Some(message) = message else { return };
					self.on_gossip_message(message);
				},
				// Drive the gossip engine (processes network events, timeouts, …);
				// its future never resolves.
				_ = (&mut self.gossip_engine).fuse() => {},
			}
		}
	}

	/// Rebuilds the authority set and deals when the session changed at `hash`.
	fn maybe_new_session(&mut self, hash: B::Hash) {
		let topic = dealings_topic::<HashingOf<B>>();
		let session_index = match keys::session_index(&*self.client, hash) {
			Ok(index) => index,
			Err(e) => {
				debug!(target: LOG_TARGET, "🎲 DKG: session index unavailable at {hash:?}: {e}");
				return
			},
		};

		if self.current_session == Some(session_index) {
			return
		}

		let authority_set =
			match keys::queued_dkg_public_keys::<_, _, _, AccountId, Keys>(&*self.client, hash) {
				Ok(set) => set,
				Err(e) => {
					debug!(target: LOG_TARGET, "🎲 DKG: authority set unavailable at {hash:?}: {e}");
					return
				},
			};

		debug!(
			target: LOG_TARGET,
			"🎲 DKG: new session {session_index} with {} validators", authority_set.len()
		);
		self.gossip_validator.update_filter(session_index, authority_set.clone());
		self.current_session = Some(session_index);

		if self.dealt_session == Some(session_index) {
			return
		}

		let Some(pair) = self.dealing_pair.clone() else {
			debug!(target: LOG_TARGET, "🎲 DKG: no dealing key configured, not dealing");
			return
		};

		let Ok((_signer, our_public)) = signer_from_pair(&pair) else {
			error!(target: LOG_TARGET, "🎲 DKG: malformed own BEEFY key, not dealing");
			return
		};
		if !authority_set.contains(&our_public) {
			debug!(target: LOG_TARGET, "🎲 DKG: own key not in authority set, not dealing");
			return
		}

		match deal(&authority_set, &pair) {
			Ok((transcript, public)) => {
				let mut dealing_bytes = Vec::new();
				if let Err(e) = transcript.serialize_compressed(&mut dealing_bytes) {
					error!(target: LOG_TARGET, "🎲 DKG: failed to serialize transcript: {e}");
					return
				}
				let payload = dealing_sign_payload(session_index, &dealing_bytes);
				let signature =
					match bls_crypto::sign_with_store(&self.keystore, &public, &payload) {
						Ok(Some(sig)) => sig,
						Ok(None) => {
							error!(target: LOG_TARGET, "🎲 DKG: BEEFY key not in keystore");
							return
						},
						Err(e) => {
							error!(target: LOG_TARGET, "🎲 DKG: keystore signing failed: {e}");
							return
						},
					};

				let message = GossipMessage::Dealing(DealingMessage {
					session_index,
					dealing: Dealing { dealing: dealing_bytes },
					id: public,
					signature,
				});
				self.gossip_engine.gossip_message(topic, message.encode(), true);
				self.dealt_session = Some(session_index);
				info!(target: LOG_TARGET, "🎲 DKG: dealt for session {session_index}");
			},
			Err(e) => error!(target: LOG_TARGET, "🎲 DKG: dealing failed: {e}"),
		}
	}

	/// Collects a validated dealing received over gossip.
	fn on_gossip_message(&mut self, message: sc_network_gossip::TopicNotification) {
		let mut data = &message.message[..];
		let Ok(gossip) = GossipMessage::decode(&mut data) else {
			return
		};
		let Some(dealing) = gossip.unwrap_dealing() else { return };
		debug!(
			target: LOG_TARGET,
			"🎲 DKG: collected dealing for session {} from {}",
			dealing.session_index,
			dealing.id
		);
		self.collected.push((dealing.session_index, dealing.dealing));
		self.collection_stats.dealings.fetch_add(1, Ordering::Relaxed);
		self.collection_stats.latest_session.fetch_max(dealing.session_index, Ordering::Relaxed);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_pair(seed: u8) -> sp_core::ecdsa_bls381::Pair {
		use sp_core::Pair as _;
		sp_core::ecdsa_bls381::Pair::from_seed(&[seed; 32])
	}

	#[test]
	fn signer_from_pair_recovers_scalar_and_keys() {
		let pair = test_pair(7);
		let (signer, public) = signer_from_pair(&pair).expect("conversion succeeds");

		// The extracted scalar reproduces the paired public key (checked inside),
		// and the a-DKG public keys match the BEEFY key's BLS components.
		assert_eq!(bls_crypto::to_ark_g2(&public), Some(signer.bls_pk_g2));
		assert_eq!(bls_crypto::to_ark_g1(&public), Some(signer.bls_pk_g1));
	}

	#[test]
	fn deal_produces_verifiable_transcript() {
		let pairs = (1u8..=4).map(test_pair).collect::<Vec<_>>();
		let authority_set = pairs
			.iter()
			.map(|p| {
				signer_from_pair(p).map(|(_, public)| public).expect("conversion succeeds")
			})
			.collect::<BTreeSet<_>>();

		let (transcript, public) =
			deal(&authority_set, &pairs[0]).expect("dealing succeeds");

		// The transcript passes the a-DKG's own deterministic checks, and the
		// dealer is bound to our BEEFY key's G1 component.
		for (receipt, _w) in &transcript.receipts {
			receipt.verify_all_sigs().expect("receipt signatures verify");
		}
		transcript.check_consistency().expect("consistent transcript");
		assert_eq!(transcript.list_dealers(), vec![bls_crypto::to_ark_g1(&public).unwrap()]);
	}
}


