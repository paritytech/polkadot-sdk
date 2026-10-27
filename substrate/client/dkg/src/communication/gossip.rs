use crate::{
	communication::{benefit, cost, peers::KnownPeers, notification::Dealing},
    LOG_TARGET,
};

use sp_runtime::traits::Hash;

use std::{fmt::Display, sync::Arc, time::Duration};

use log::debug;

use sp_application_crypto::{AppPublic, RuntimeAppPublic};

use codec::{Encode, Decode, DecodeWithMemTracking};
use scale_info::TypeInfo;

use sc_network::{NetworkPeers, ReputationChange};
use sc_network_types::PeerId;

use parking_lot::{Mutex, RwLock};
use wasm_timer::Instant;

const REBROADCAST_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
pub(super) enum Action<H> {
	// repropagate under given topic, to the given peers, applying cost/benefit to originator.
	Keep(H, ReputationChange),
	// discard, applying cost/benefit to originator.
	Discard(ReputationChange),
	// ignore, no cost/benefit applied to originator.
	DiscardNoReport,
}

/// An outcome of examining a message.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Consider {
	/// Accept the message.
	Accept,
	/// Message is for a past session.
	RejectPast,
	/// Message is for a future session.
	RejectFuture,
	/// Sender's authority id is not in the current session's set.
	UnknownParticipant,
	/// Message cannot be evaluated. Reject.
	CannotEvaluate,
}

/// DKG Dealing Message.
///
/// A Dealing message is the dealing of a single participant in our DKG scheme with a signature attached
#[derive(Clone, Debug, Decode, DecodeWithMemTracking, Encode, PartialEq, TypeInfo)]
pub struct DealingMessage<Dealing, Id, Signature> {
	/// Session index this dealing belongs to.
	pub session_index: u32,
	/// Commit to information extracted from a finalized block
	pub dealing: Dealing,
    /// Node authority id
	pub id: Id,
	/// Node signature
	pub signature: Signature,
}

/// DKG gossip message type that gets encoded and sent on the network.
///
/// Concrete over the BEEFY paired (ECDSA,BLS12-381) crypto reused by the DKG.
#[cfg(feature = "bls-experimental")]
#[derive(Debug, Encode, Decode)]
pub(crate) enum GossipMessage {
	/// DKG message with commitment and single signature.
	Dealing(DealingMessage<Dealing<Vec<u8>>, bls_crypto::AuthorityId, bls_crypto::Signature>),
    // TODO: Add another message?
    Temp,
}

pub trait AuthorityIdBound:
    Ord
	+ AppPublic
    + Display
    + RuntimeAppPublic
{

}

#[cfg(feature = "bls-experimental")]
impl GossipMessage {
    pub fn unwrap_dealing(
        self
    ) -> Option<DealingMessage<Dealing<Vec<u8>>, bls_crypto::AuthorityId, bls_crypto::Signature>> {
        match self {
            GossipMessage::Dealing(dealing) => Some(dealing),
            GossipMessage::Temp => None,
        }
    }
}

/// Gossip engine dealings messages topic
pub(crate) fn dealings_topic<H: Hash>() -> H::Output
where
	H: Hash,
{
	H::hash_of(b"dkg-dealing")
}

/// DKG cryptographic types and helpers.
///
/// The DKG reuses the validators' **BEEFY session keys** (decision: Alistair,
/// 2026-09-29): substrate's experimental paired (ECDSA,BLS12-381) crypto
/// (`sp_consensus_beefy::ecdsa_bls_crypto`), key type `*b"beef"`. The paired
/// public key is `[ecdsa (33B) || pk_in_G1 (48B) || pk_in_G2 (96B)]` (the
/// w3f-bls `DoublePublicKey` serializes its G1 component first — verified
/// empirically against real keys). The G2 component is the validator's signer
/// public key in the a-DKG (`adkg-vrf`); the G1 component is the dealer public
/// key bound by the `ContributionReceipt` proof of possession.
///
/// This module basically introduces four crypto types:
/// - `bls_crypto::Pair`
/// - `bls_crypto::Public`
/// - `bls_crypto::Signature`
/// - `bls_crypto::AuthorityId`
///
/// Your code should use the above types as concrete types for all crypto related
/// functionality.
#[cfg(feature = "bls-experimental")]
pub mod bls_crypto {
	use super::AuthorityIdBound;
	pub use sp_consensus_beefy::ecdsa_bls_crypto::*;
	pub use sp_core::crypto::key_types::BEEFY as BEEFY_KEY_TYPE;

	use sp_core::crypto::ByteArray;

	impl AuthorityIdBound for AuthorityId {}

	/// Serialized size of the ECDSA component of a paired (ECDSA,BLS12-381) key.
	pub const ECDSA_SIZE: usize = sp_core::ecdsa::PUBLIC_KEY_SERIALIZED_SIZE;
	/// Serialized size of the G2 component of a (double) BLS12-381 public key.
	pub const G2_COMPRESSED_SIZE: usize = 96;
	/// Serialized size of the G1 component of a (double) BLS12-381 public key.
	pub const G1_COMPRESSED_SIZE: usize = 48;

	/// Extracts the G1 component of a validator's BEEFY paired public key,
	/// i.e. the dealer public key bound by the `ContributionReceipt` PoP.
	///
	/// Layout: `[ecdsa (33B) || pk_in_G1 (48B) || pk_in_G2 (96B)]`.
	pub fn to_ark_g1(public: &Public) -> Option<ark_bls12_381::G1Affine> {
		use ark_serialize::CanonicalDeserialize;
		let bytes: &[u8] = AsRef::<[u8]>::as_ref(public);
		ark_bls12_381::G1Affine::deserialize_compressed(
			&bytes[ECDSA_SIZE..ECDSA_SIZE + G1_COMPRESSED_SIZE],
		)
		.ok()
	}

	/// Extracts the G2 component of a validator's BEEFY paired public key,
	/// i.e. the signer public key used by the a-DKG (`adkg-vrf`).
	pub fn to_ark_g2(public: &Public) -> Option<ark_bls12_381::G2Affine> {
		use ark_serialize::CanonicalDeserialize;
		let bytes: &[u8] = AsRef::<[u8]>::as_ref(public);
		ark_bls12_381::G2Affine::deserialize_compressed(
			&bytes[ECDSA_SIZE + G1_COMPRESSED_SIZE..ECDSA_SIZE + G1_COMPRESSED_SIZE + G2_COMPRESSED_SIZE],
		)
		.ok()
	}

	/// Returns all BEEFY paired (ECDSA,BLS12-381) public keys stored in the keystore.
	pub fn public_keys(store: &sp_keystore::KeystorePtr) -> Vec<Public> {
		store
			.ecdsa_bls381_public_keys(BEEFY_KEY_TYPE)
			.into_iter()
			.filter_map(|p| Public::from_slice(p.as_ref()).ok())
			.collect()
	}

	/// Signs a DKG dealing message with the validator's BEEFY paired key from the
	/// keystore. Returns `Ok(None)` if the key is not in the keystore.
	pub fn sign_with_store(
		store: &sp_keystore::KeystorePtr,
		public: &Public,
		msg: &[u8],
	) -> Result<Option<Signature>, sp_keystore::Error> {
		let raw_public = sp_core::ecdsa_bls381::Public::from_slice(AsRef::<[u8]>::as_ref(public))
			.map_err(|_| sp_keystore::Error::ValidationError("invalid BEEFY public key".into()))?;
		// Keccak256 hashing of the ECDSA half, consistent with BEEFY verification
		// (`BeefyAuthorityId::verify` uses `verify_with_hasher::<Keccak256>`).
		let sig = store.ecdsa_bls381_sign_with_keccak256(BEEFY_KEY_TYPE, &raw_public, msg)?;
		Ok(sig.and_then(|s| Signature::from_slice(s.as_ref()).ok()))
	}
}


/// The DKG gossip filter: the current session index and DKG authority set.
///
/// Dealings for past sessions are outdated, dealings for future sessions are
/// rejected, and dealings from keys outside the set are unknown participants.
#[cfg(feature = "bls-experimental")]
#[derive(Debug)]
pub(crate) struct Filter {
	session_index: u32,
	authority_set: std::collections::BTreeSet<bls_crypto::Public>,
}

#[cfg(feature = "bls-experimental")]
impl Filter {
	pub(crate) fn new(session_index: u32, authority_set: std::collections::BTreeSet<bls_crypto::Public>) -> Self {
		Self { session_index, authority_set }
	}

	pub(crate) fn session_index(&self) -> u32 {
		self.session_index
	}

	fn consider(&self, session_index: u32, id: &bls_crypto::Public) -> Consider {
		if session_index < self.session_index {
			return Consider::RejectPast
		}
		if session_index > self.session_index {
			return Consider::RejectFuture
		}
		if !self.authority_set.contains(id) {
			return Consider::UnknownParticipant
		}
		Consider::Accept
	}
}

/// What the dealer signs over a dealing: a domain-separated, session-scoped
/// binding of the serialized a-DKG transcript.
#[cfg(feature = "bls-experimental")]
pub(crate) fn dealing_sign_payload(session_index: u32, dealing: &[u8]) -> Vec<u8> {
	(b"dkg/dealing", session_index, dealing).encode()
}

/// Validates a gossiped dealing against the current filter.
///
/// Checks (in order): session index vs. the current session, sender membership
/// in the authority set, and the signature over [`dealing_sign_payload`].
#[cfg(feature = "bls-experimental")]
pub(crate) fn validate_dealing<H: Hash>(
	filter: Option<&Filter>,
	msg: DealingMessage<
		Dealing<Vec<u8>>,
		bls_crypto::Public,
		bls_crypto::Signature,
	>,
	sender: &PeerId,
) -> Action<H::Output> {
	use sp_application_crypto::RuntimeAppPublic;

	let filter = match filter {
		Some(f) => f,
		None => return Action::DiscardNoReport,
	};

	match filter.consider(msg.session_index, &msg.id) {
		Consider::RejectPast => return Action::Discard(cost::OUTDATED_MESSAGE),
		Consider::RejectFuture => return Action::Discard(cost::FUTURE_MESSAGE),
		Consider::UnknownParticipant => return Action::Discard(cost::UNKNOWN_PARTICIPANT),
		// When we can't evaluate, it's our fault (e.g. filter not initialized yet),
		// we discard the message without punishing or rewarding the sending peer.
		Consider::CannotEvaluate => return Action::DiscardNoReport,
		Consider::Accept => {},
	}

	// NOTE: `RuntimeAppPublic::verify` cannot be used here — its impl for paired
	// (ECDSA,BLS) keys is a dummy returning `false`; BEEFY's own trait does the
	// real verification (Keccak for the ECDSA half, BLS pairing for the other).
	use sp_consensus_beefy::BeefyAuthorityId;
	let payload = dealing_sign_payload(msg.session_index, &msg.dealing.dealing);
	if BeefyAuthorityId::verify(&msg.id, &msg.signature, &payload) {
		Action::Keep(dealings_topic::<H>(), benefit::DEALING_MESSAGE)
	} else {
		debug!(
			target: LOG_TARGET,
			"🎲 Bad signature on DKG dealing message from: {sender:?}"
		);
		Action::Discard(cost::BAD_SIGNATURE)
	}
}

/// DKG gossip validator
///
/// Validates DKG gossip messages; the filter is updated by the worker on
/// session changes.
#[cfg(feature = "bls-experimental")]
pub(crate) struct GossipValidator<H, N>
where
	H: Hash,
{
	gossip_filter: RwLock<Option<Filter>>,
	next_rebroadcast: Mutex<Instant>,
	known_peers: Arc<Mutex<KnownPeers<H>>>,
	network: N,
}

#[cfg(feature = "bls-experimental")]
impl<H, N> GossipValidator<H, N>
where
	H: Hash,
	N: NetworkPeers + Send + Sync,
{
	pub(crate) fn new(network: N, known_peers: Arc<Mutex<KnownPeers<H>>>) -> Self {
		Self {
			gossip_filter: RwLock::new(None),
			next_rebroadcast: Mutex::new(Instant::now() + REBROADCAST_AFTER),
			known_peers,
			network,
		}
	}

	/// Point the validator at a new session: session index + authority set.
	pub(crate) fn update_filter(
		&self,
		session_index: u32,
		authority_set: std::collections::BTreeSet<bls_crypto::Public>,
	) {
		let mut filter = self.gossip_filter.write();
		*filter = Some(Filter::new(session_index, authority_set));
	}
}

#[cfg(feature = "bls-experimental")]
impl<B, H, N> sc_network_gossip::Validator<B> for GossipValidator<H, N>
where
	B: sp_runtime::traits::Block,
	H: Hash<Output = B::Hash> + Send + Sync,
	N: NetworkPeers + Send + Sync,
{
	fn new_peer(&self, _context: &mut dyn sc_network_gossip::ValidatorContext<B>, who: &PeerId, _role: sc_network::ObservedRole) {
		self.known_peers.lock().note_vote_for(*who, H::Output::default());
	}

	fn peer_disconnected(&self, _context: &mut dyn sc_network_gossip::ValidatorContext<B>, who: &PeerId) {
		self.known_peers.lock().remove(who);
	}

	fn validate(
		&self,
		_context: &mut dyn sc_network_gossip::ValidatorContext<B>,
		sender: &PeerId,
		mut data: &[u8],
	) -> sc_network_gossip::ValidationResult<B::Hash> {
		let message = match GossipMessage::decode(&mut data) {
			Ok(m) => m,
			Err(_) => {
				debug!(target: LOG_TARGET, "🎲 Undecodable DKG gossip message from: {sender:?}");
				return sc_network_gossip::ValidationResult::Discard
			},
		};

		let dealing = match message.unwrap_dealing() {
			Some(d) => d,
			None => return sc_network_gossip::ValidationResult::Discard,
		};

		let action = validate_dealing::<H>(self.gossip_filter.read().as_ref(), dealing, sender);

		match action {
			Action::Keep(topic, cb) => {
				self.network.report_peer(*sender, cb);
				sc_network_gossip::ValidationResult::ProcessAndKeep(topic)
			},
			Action::Discard(cb) => {
				self.network.report_peer(*sender, cb);
				sc_network_gossip::ValidationResult::Discard
			},
			Action::DiscardNoReport => sc_network_gossip::ValidationResult::Discard,
		}
	}

	fn message_expired<'a>(&'a self) -> Box<dyn FnMut(B::Hash, &[u8]) -> bool + 'a> {
		let filter = self.gossip_filter.read();
		let session_index = filter.as_ref().map(|f| f.session_index());
		Box::new(move |_topic, mut data: &[u8]| {
			let Some(current) = session_index else { return false };
			let Ok(message) = GossipMessage::decode(&mut data) else {
				return true
			};
			message
				.unwrap_dealing()
				.map(|dealing| dealing.session_index < current)
				.unwrap_or(true)
		})
	}
}
#[cfg(all(test, feature = "bls-experimental"))]
mod tests {
	use super::bls_crypto;
	use ark_bls12_381::{Fr, G1Projective, G2Projective};
	use ark_ec::{CurveGroup, PrimeGroup};
	use ark_ff::UniformRand;
	use ark_serialize::CanonicalSerialize;
	use sp_core::crypto::{ByteArray, UncheckedFrom};

	/// The paired (ECDSA,BLS12-381) public key layout is
	/// `[ecdsa (33B) || pk_in_G1 (48B) || pk_in_G2 (96B)]` (G1 first!);
	/// `to_ark_g1`/`to_ark_g2` must extract exactly the BLS components used by the
	/// a-DKG, including for keys produced by sp-core itself.
	#[test]
	fn ark_extraction_matches_paired_public_key_layout() {
		let rng = &mut ark_std::test_rng();
		let sk = Fr::rand(rng);
		let pk_g1 = (G1Projective::generator() * sk).into_affine();
		let pk_g2 = (G2Projective::generator() * sk).into_affine();

		let mut raw = [0u8; 177];
		// bytes[..33] is the ECDSA component; content is opaque here.
		pk_g1.serialize_compressed(&mut raw[33..81]).unwrap();
		pk_g2.serialize_compressed(&mut raw[81..]).unwrap();

		let public = bls_crypto::Public::unchecked_from(raw);
		assert_eq!(bls_crypto::to_ark_g1(&public), Some(pk_g1));
		assert_eq!(bls_crypto::to_ark_g2(&public), Some(pk_g2));
	}

	/// Regression test: extraction must agree with sp-core's own key generation
	/// (the layout was verified against real keys after an initial wrong guess).
	#[test]
	fn ark_extraction_matches_sp_core_generated_keys() {
		use sp_core::Pair as _;
		let pair = sp_core::ecdsa_bls381::Pair::from_seed(&[7u8; 32]);
		let public =
			bls_crypto::Public::from_slice(AsRef::<[u8]>::as_ref(&pair.public())).unwrap();
		assert!(bls_crypto::to_ark_g1(&public).is_some());
		assert!(bls_crypto::to_ark_g2(&public).is_some());
	}

	mod validation {
		use crate::communication::gossip::{
			bls_crypto, dealing_sign_payload, validate_dealing, DealingMessage, Filter,
		};
		use crate::communication::notification::Dealing;
		use sp_core::{crypto::ByteArray, Pair as _};
		use sp_runtime::traits::BlakeTwo256;

		fn test_pair(seed: u8) -> sp_core::ecdsa_bls381::Pair {
			sp_core::ecdsa_bls381::Pair::from_seed(&[seed; 32])
		}

		fn test_public(pair: &sp_core::ecdsa_bls381::Pair) -> bls_crypto::Public {
			bls_crypto::Public::from_slice(pair.public().as_ref()).unwrap()
		}

		fn signed_dealing(
			session_index: u32,
			dealing_bytes: &[u8],
			pair: &sp_core::ecdsa_bls381::Pair,
		) -> DealingMessage<Dealing<Vec<u8>>, bls_crypto::Public, bls_crypto::Signature> {
			let payload = dealing_sign_payload(session_index, dealing_bytes);
			let signature = pair.sign_with_hasher::<sp_runtime::traits::Keccak256>(&payload);
			DealingMessage {
				session_index,
				dealing: Dealing { dealing: dealing_bytes.to_vec() },
				id: test_public(pair),
				signature: bls_crypto::Signature::from_slice(signature.as_ref()).unwrap(),
			}
		}

		fn peer() -> sc_network_types::PeerId {
			sc_network_types::PeerId::random()
		}

		#[test]
		fn accepts_signed_dealing_from_set_member() {
			let pair = test_pair(1);
			let set = std::collections::BTreeSet::from([test_public(&pair)]);
			let filter = Filter::new(3, set);
			let msg = signed_dealing(3, b"fake-transcript", &pair);

			match validate_dealing::<BlakeTwo256>(Some(&filter), msg, &peer()) {
				crate::communication::gossip::Action::Keep(_, _) => {},
				other => panic!("expected Keep, got {other:?}"),
			}
		}

		#[test]
		fn discards_past_future_and_unknown() {
			let pair = test_pair(1);
			let set = std::collections::BTreeSet::from([test_public(&pair)]);
			let filter = Filter::new(3, set);

			// Past session.
			let msg = signed_dealing(2, b"fake-transcript", &pair);
			assert!(matches!(
				validate_dealing::<BlakeTwo256>(Some(&filter), msg, &peer()),
				crate::communication::gossip::Action::Discard(_)
			));

			// Future session.
			let msg = signed_dealing(4, b"fake-transcript", &pair);
			assert!(matches!(
				validate_dealing::<BlakeTwo256>(Some(&filter), msg, &peer()),
				crate::communication::gossip::Action::Discard(_)
			));

			// Signer not in the authority set.
			let outsider = test_pair(9);
			let msg = signed_dealing(3, b"fake-transcript", &outsider);
			assert!(matches!(
				validate_dealing::<BlakeTwo256>(Some(&filter), msg, &peer()),
				crate::communication::gossip::Action::Discard(_)
			));

			// Uninitialized filter: discard without report.
			let msg = signed_dealing(3, b"fake-transcript", &pair);
			assert!(matches!(
				validate_dealing::<BlakeTwo256>(None, msg, &peer()),
				crate::communication::gossip::Action::DiscardNoReport
			));
		}

		#[test]
		fn discards_bad_signature() {
			let pair = test_pair(1);
			let set = std::collections::BTreeSet::from([test_public(&pair)]);
			let filter = Filter::new(3, set);

			let mut msg = signed_dealing(3, b"fake-transcript", &pair);
			// Tamper with the dealing after signing.
			msg.dealing.dealing = b"other-transcript".to_vec();

			assert!(matches!(
				validate_dealing::<BlakeTwo256>(Some(&filter), msg, &peer()),
				crate::communication::gossip::Action::Discard(_)
			));
		}
	}
}
