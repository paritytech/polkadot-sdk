//! On-chain aggregation of the relay-chain validators' a-DKG dealings into
//! per-session threshold keys.
//!
//! The dealers are the relay-chain validators. Every relay session is one DKG
//! round: during relay session `s` the validators queued in it (relay
//! `Beefy::NextAuthorities`, the validator set of session `s + 1`) deal, and
//! each sends its dealing to this parachain as an unsigned
//! [`Call::submit_dealing`]. A dealing authenticates itself: its contribution
//! receipt is a proof of possession binding it to the dealer key, the G1
//! component of the dealer's BEEFY (ECDSA,BLS12-381) session key, so the pallet
//! only checks that the dealer is in the round's validator set.
//!
//! The pallet follows the relay chain through the relay state proof: the
//! collator includes the relay keys from [`relay_keys_to_prove`], and
//! [`OnSystemEvent::on_relay_state_proof`] reads the relay session index and,
//! once per session, the queued validators' BEEFY keys. When the relay session
//! advances the round ends, storing the threshold key under
//! [`ThresholdKeys`]`[s + 1]` if enough validators dealt.

pub use pallet::*;

use alloc::{vec, vec::Vec};

use cumulus_pallet_parachain_system::{relay_state_snapshot::RelayChainStateProof, OnSystemEvent};
use frame_support::{storage::storage_prefix, traits::Get, weights::Weight};

/// Index of a session (same as `sp_staking::SessionIndex`).
pub type SessionIndex = u32;

/// A relay-chain validator's BEEFY key (paired ECDSA,BLS12-381), as stored in
/// the relay chain's `Beefy::NextAuthorities`.
pub type BeefyId = sp_consensus_beefy::ecdsa_bls_crypto::AuthorityId;

/// A dealer key: the compressed G1 component of a BEEFY paired key.
pub type DealerKey = [u8; G1_COMPRESSED_SIZE];

/// Relay-chain storage key of `Session::CurrentIndex`.
pub fn relay_session_index_key() -> Vec<u8> {
	storage_prefix(b"Session", b"CurrentIndex").to_vec()
}

/// Relay-chain storage key of `Beefy::NextAuthorities`: the BEEFY keys of the
/// validator set queued for the next session.
pub fn relay_next_authorities_key() -> Vec<u8> {
	storage_prefix(b"Beefy", b"NextAuthorities").to_vec()
}

/// The relay-chain storage keys the collator must include in the relay state
/// proof: always the session index, and the queued validators' BEEFY keys
/// until the round of the latest relay session has started (so the dealer set
/// is in the proof once per session, not in every block).
pub fn relay_keys_to_prove<T: Config>() -> Vec<Vec<u8>> {
	let mut keys = vec![relay_session_index_key()];
	if Pallet::<T>::awaiting_dealers() {
		keys.push(relay_next_authorities_key());
	}
	keys
}

impl<T: Config> OnSystemEvent for Pallet<T> {
	fn on_validation_data(_: &cumulus_primitives_core::PersistedValidationData) {}

	fn on_validation_code_applied() {}

	fn on_relay_state_proof(relay_state_proof: &RelayChainStateProof) -> Weight {
		// An absent `CurrentIndex` is the relay genesis session; an error means
		// the collator did not include the key.
		let Ok(session) =
			relay_state_proof.read_entry::<SessionIndex>(&relay_session_index_key(), Some(0))
		else {
			return Weight::zero();
		};
		// `Err` when the key was not requested for this block.
		let next_authorities = relay_state_proof
			.read_optional_entry::<Vec<BeefyId>>(&relay_next_authorities_key())
			.ok()
			.flatten();
		Self::on_relay_session(session, next_authorities);
		// TODO: benchmark (ending a round decodes the aggregate).
		T::DbWeight::get().reads_writes(4, 5)
	}
}

/// Minimum number of distinct dealers for a round dealing to `participants`
/// validators: `t_dkg = f + 1` with `f = (n - 1) / 3`, the BFT parameters the
/// DKG worker deals with (`sc_dkg::worker::deal`). With `f + 1` dealers at
/// least one honest dealer contributed to the key.
pub fn dealing_threshold(participants: u32) -> u32 {
	participants.saturating_sub(1) / 3 + 1
}

/// Serialized size of the ECDSA component of a BEEFY paired public key.
const ECDSA_SIZE: usize = 33;
/// Serialized size of the G2 component of a (double) BLS12-381 public key.
const G2_COMPRESSED_SIZE: usize = 96;
/// Serialized size of the G1 component of a (double) BLS12-381 public key.
const G1_COMPRESSED_SIZE: usize = 48;

/// Returns the G1 component (the a-DKG dealer key) of a serialized BEEFY paired
/// (ECDSA,BLS12-381) session key, i.e. `raw[33..81]` of the layout
/// `[ecdsa (33B) || pk_in_G1 (48B) || pk_in_G2 (96B)]` — the w3f-bls
/// `DoublePublicKey` serializes its G1 component first (verified empirically).
///
/// Returns `None` for anything but a full 177-byte paired public key.
pub fn session_key_g1(raw: &[u8]) -> Option<&[u8]> {
	if raw.len() != ECDSA_SIZE + G1_COMPRESSED_SIZE + G2_COMPRESSED_SIZE {
		return None;
	}
	raw.get(ECDSA_SIZE..ECDSA_SIZE + G1_COMPRESSED_SIZE)
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use alloc::{vec, vec::Vec};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::SaturatedConversion;

	use adkg_vrf::dkg::{transcript::Transcript, Dkg};
	use ark_bls12_381::Bls12_381;
	use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

	use super::{dealing_threshold, session_key_g1, BeefyId, DealerKey, SessionIndex};

	type DkgTranscript = Transcript<Bls12_381>;

	/// How long (in blocks) a dealing stays valid in the transaction pool.
	const DEALING_LONGEVITY: u64 = 64;

	/// The DKG round in progress.
	///
	/// Round `session` collects the dealings made during relay session `session`
	/// by the validator set queued in it, i.e. the relay validators of session
	/// `session + 1`.
	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct Round {
		/// The relay session in which the round's dealings are made.
		pub session: SessionIndex,
		/// The dealers' keys, in relay validator order.
		pub dealers: Vec<DealerKey>,
		/// Whether each dealer already dealt in this round.
		pub dealt: Vec<bool>,
	}

	impl Round {
		fn dealer_index(&self, dealer: &[u8]) -> Option<usize> {
			self.dealers.iter().position(|key| key[..] == *dealer)
		}
	}

	/// The threshold key produced by a successful DKG round.
	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct ThresholdKey {
		/// Number of distinct dealers aggregated into the key.
		pub dealers: u32,
		/// Compressed serialized `adkg_vrf::pvss::SecretSharing<Bls12_381>`: the
		/// group public key `c = f(0).g1`, the binding keys `h1`/`h2` and the
		/// per-signer `bgpk`s. `ThresholdVk::from_share` derives the VUF
		/// verification key from it.
		pub secret_sharing: Vec<u8>,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Number of most recent sessions whose threshold keys are kept in
		/// [`ThresholdKeys`].
		#[pallet::constant]
		type KeyHistoryDepth: Get<SessionIndex>;

		/// Transaction pool priority of dealings.
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The round of relay session `session` started with `participants` dealers.
		RoundStarted { session: SessionIndex, participants: u32 },
		/// The dealing of the `dealer`-th validator was merged into round `session`.
		DealingAdded { session: SessionIndex, dealer: u32 },
		/// A round ended with enough dealers: the threshold key of the validator
		/// set of `session` is in [`ThresholdKeys`]; `c` is the group public key.
		KeyGenerated { session: SessionIndex, dealers: u32, c: Vec<u8> },
		/// A round ended with fewer than `threshold` dealers: the validator set of
		/// `session` has no threshold key.
		KeyGenerationFailed { session: SessionIndex, dealers: u32, threshold: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		VerificationFailed,
		DeserializationFailed,
		SerializationFailed,
		/// No DKG round is in progress (the relay validator set is not known yet).
		NoRound,
		/// The dealing was made for another relay session than the current round's.
		WrongSession,
		/// The transcript has no contribution receipts.
		NoDealers,
		/// The transcript must be a single validator's fresh dealing.
		NotSingleDealer,
		/// The dealer is not a validator of the current round.
		UnknownDealer,
		/// The dealer already dealt in the current round.
		DuplicateDealer,
	}

	/// The latest relay session index seen in the relay state proof.
	#[pallet::storage]
	pub type RelaySession<T: Config> = StorageValue<_, SessionIndex, OptionQuery>;

	/// The DKG round in progress.
	#[pallet::storage]
	pub type CurrentRound<T: Config> = StorageValue<_, Round, OptionQuery>;

	/// Aggregate of the current round's dealings; reset with every new round.
	#[pallet::storage]
	pub type AggregatedTranscript<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Threshold keys by the relay session whose validator set holds them, for
	/// the last [`Config::KeyHistoryDepth`] sessions.
	#[pallet::storage]
	pub type ThresholdKeys<T: Config> =
		StorageMap<_, Twox64Concat, SessionIndex, ThresholdKey, OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a relay validator's dealing for the current round (unsigned).
		///
		/// `session_index` is the relay session the dealing was made in and must
		/// be the current round's: a dealing is made for the validator set queued
		/// in its session, so one landing after the next relay session started is
		/// stale. The transcript must be a single validator's fresh dealing; its
		/// receipt's proof of possession binds it to the dealer key, which must
		/// belong to a validator of the round that has not dealt yet.
		#[pallet::call_index(0)]
		#[pallet::weight(10_000_000)] // TODO: benchmark
		pub fn submit_dealing(
			origin: OriginFor<T>,
			session_index: SessionIndex,
			transcript_bytes: Vec<u8>,
		) -> DispatchResult {
			ensure_none(origin)?;
			let (dealer, new_transcript) = Self::check_dealing(session_index, &transcript_bytes)?;

			let mut aggregated = match AggregatedTranscript::<T>::get() {
				Some(existing_bytes) => {
					let existing = DkgTranscript::deserialize_compressed(&existing_bytes[..])
						.map_err(|_| Error::<T>::DeserializationFailed)?;
					Dkg::<Bls12_381>::aggregate(vec![existing, new_transcript])
				},
				None => new_transcript,
			};
			// `Dkg::aggregate` collects the receipts through a `HashMap` whose
			// iteration order depends on a per-hasher seed (foldhash seeds from the
			// stack pointer, which differs between block authoring and import).
			// Sort them into a canonical order so the stored aggregate, and hence
			// the storage root, is deterministic; dealers are unique per round.
			aggregated
				.receipts
				.sort_by_key(|(receipt, _)| (receipt.dealer_pk.x, receipt.dealer_pk.y));

			let mut aggregated_bytes = Vec::new();
			aggregated
				.serialize_compressed(&mut aggregated_bytes)
				.map_err(|_| Error::<T>::SerializationFailed)?;
			AggregatedTranscript::<T>::put(aggregated_bytes);
			CurrentRound::<T>::mutate(|round| {
				if let Some(flag) = round.as_mut().and_then(|round| round.dealt.get_mut(dealer)) {
					*flag = true;
				}
			});

			Self::deposit_event(Event::DealingAdded {
				session: session_index,
				dealer: dealer.saturated_into(),
			});
			Ok(())
		}
	}

	#[allow(deprecated)]
	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			// Dealings come from the relay validators over RPC (external source).
			let Call::submit_dealing { session_index, transcript_bytes } = call else {
				return InvalidTransaction::Call.into();
			};
			let (dealer, _) = Self::check_dealing(*session_index, transcript_bytes).map_err(
				|error| match error {
					Error::<T>::NoRound => InvalidTransaction::Future,
					Error::<T>::WrongSession
						if CurrentRound::<T>::get()
							.is_some_and(|round| *session_index > round.session) =>
					{
						InvalidTransaction::Future
					},
					Error::<T>::WrongSession | Error::<T>::DuplicateDealer => {
						InvalidTransaction::Stale
					},
					Error::<T>::UnknownDealer => InvalidTransaction::BadSigner,
					_ => InvalidTransaction::BadProof,
				},
			)?;

			ValidTransaction::with_tag_prefix("KeyGenDealing")
				.priority(T::UnsignedPriority::get())
				.and_provides((session_index, dealer as u32))
				.longevity(DEALING_LONGEVITY)
				.propagate(true)
				.build()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Whether the dealer set of the latest relay session still has to be read
		/// from the relay state proof.
		pub fn awaiting_dealers() -> bool {
			RelaySession::<T>::get() != CurrentRound::<T>::get().map(|round| round.session)
		}

		/// Follows the relay chain: a new relay `session` ends the current round,
		/// and the next round starts once the queued validators' BEEFY keys
		/// (`next_authorities`, read from the same relay state) are available.
		pub(crate) fn on_relay_session(
			session: SessionIndex,
			next_authorities: Option<Vec<BeefyId>>,
		) {
			if RelaySession::<T>::get() != Some(session) {
				RelaySession::<T>::put(session);
				if CurrentRound::<T>::get().is_some_and(|round| round.session < session) {
					Self::end_round();
				}
			}
			if let (true, Some(authorities)) = (Self::awaiting_dealers(), next_authorities) {
				Self::start_round(session, &authorities);
			}
		}

		/// Starts round `session`, dealt by the holders of `authorities`.
		fn start_round(session: SessionIndex, authorities: &[BeefyId]) {
			let dealers = authorities
				.iter()
				.filter_map(|authority| session_key_g1(authority.as_ref())?.try_into().ok())
				.collect::<Vec<DealerKey>>();
			let participants = dealers.len().saturated_into();
			AggregatedTranscript::<T>::kill();
			CurrentRound::<T>::put(Round { session, dealt: vec![false; dealers.len()], dealers });
			Self::deposit_event(Event::RoundStarted { session, participants });
		}

		/// Ends the current round: stores its threshold key if enough validators
		/// dealt, and prunes keys older than [`Config::KeyHistoryDepth`] sessions.
		fn end_round() {
			let Some(round) = CurrentRound::<T>::take() else { return };
			let session = round.session.saturating_add(1);
			let threshold = dealing_threshold(round.dealers.len().saturated_into());

			// The aggregate is written by `submit_dealing` from validated
			// transcripts; were it ever undecodable, the round fails with 0 dealers.
			match AggregatedTranscript::<T>::take().and_then(|bytes| Self::threshold_key(&bytes)) {
				Some((key, c)) if key.dealers >= threshold => {
					Self::deposit_event(Event::KeyGenerated { session, dealers: key.dealers, c });
					ThresholdKeys::<T>::insert(session, key);
				},
				key => Self::deposit_event(Event::KeyGenerationFailed {
					session,
					dealers: key.map_or(0, |(key, _)| key.dealers),
					threshold,
				}),
			}

			if let Some(expired) = session.checked_sub(T::KeyHistoryDepth::get()) {
				ThresholdKeys::<T>::remove(expired);
			}
		}

		/// Checks a dealing for the current round; returns the dealer's index in
		/// the round and the transcript. Cheap checks run before the proof of
		/// possession, since dealings arrive unsigned from outside.
		pub(crate) fn check_dealing(
			session_index: SessionIndex,
			transcript_bytes: &[u8],
		) -> Result<(usize, DkgTranscript), Error<T>> {
			let round = CurrentRound::<T>::get().ok_or(Error::<T>::NoRound)?;
			ensure!(session_index == round.session, Error::<T>::WrongSession);

			let transcript = DkgTranscript::deserialize_compressed(transcript_bytes)
				.map_err(|_| Error::<T>::DeserializationFailed)?;

			let dealers = transcript.list_dealers();
			let dealer_pk = dealers.first().ok_or(Error::<T>::NoDealers)?;
			ensure!(dealers.iter().all(|d| d == dealer_pk), Error::<T>::NotSingleDealer);
			let mut dealer_bytes = Vec::new();
			dealer_pk
				.serialize_compressed(&mut dealer_bytes)
				.map_err(|_| Error::<T>::SerializationFailed)?;
			let dealer = round.dealer_index(&dealer_bytes).ok_or(Error::<T>::UnknownDealer)?;
			ensure!(round.dealt.get(dealer) != Some(&true), Error::<T>::DuplicateDealer);

			// TODO: full PVSS verification needs deterministic Fiat-Shamir challenges
			// (adkg-vrf `verify_with_fs`); until then only the deterministic checks run.
			for (receipt, _w) in &transcript.receipts {
				receipt.verify_all_sigs().map_err(|_| Error::<T>::VerificationFailed)?;
			}
			transcript.check_consistency().map_err(|_| Error::<T>::VerificationFailed)?;

			Ok((dealer, transcript))
		}

		/// The threshold key and the group public key `c` of a serialized aggregate.
		fn threshold_key(transcript_bytes: &[u8]) -> Option<(ThresholdKey, Vec<u8>)> {
			let transcript = DkgTranscript::deserialize_compressed(transcript_bytes).ok()?;
			let mut secret_sharing = Vec::new();
			transcript.agg_ss.payload.serialize_compressed(&mut secret_sharing).ok()?;
			let mut c = Vec::new();
			transcript.agg_ss.payload.c.serialize_compressed(&mut c).ok()?;
			let dealers = transcript.list_dealers().len().saturated_into();
			Some((ThresholdKey { dealers, secret_sharing }, c))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::sync::atomic::{AtomicU64, Ordering};

	use adkg_vrf::{bls::threshold::ThresholdVk, dkg::Dkg, pvss::SecretSharing};
	use ark_bls12_381::{Bls12_381, Fr, G1Affine, G1Projective, G2Affine, G2Projective};
	use ark_ec::{CurveGroup, PrimeGroup};
	use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
	use ark_std::rand::{rngs::StdRng, SeedableRng};
	use codec::Encode;
	use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32};
	use sp_core::{crypto::ByteArray, ConstU64};
	#[allow(deprecated)]
	use sp_runtime::traits::ValidateUnsigned;
	use sp_runtime::{
		transaction_validity::{InvalidTransaction, TransactionSource},
		BuildStorage,
	};

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			KeyGen: crate::key_gen,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = frame_system::mocking::MockBlock<Test>;
	}

	impl Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type KeyHistoryDepth = ConstU32<2>;
		type UnsignedPriority = ConstU64<100>;
	}

	/// The relay validators `1..=VALIDATORS`; validator `v`'s BLS secret is
	/// `Fr::from(v)`. With `n = 4`, `f = 1` and 2 dealers are needed.
	const VALIDATORS: u64 = 4;

	fn g1(sk: Fr) -> G1Affine {
		(G1Projective::generator() * sk).into_affine()
	}

	fn g2(sk: Fr) -> G2Affine {
		(G2Projective::generator() * sk).into_affine()
	}

	/// Builds a BEEFY paired key `[ecdsa (33B) || pk_G1 (48B) || pk_G2 (96B)]` for
	/// the given BLS secret. The ECDSA component is opaque filler (irrelevant to
	/// the DKG).
	fn paired_key_bytes(sk: Fr) -> Vec<u8> {
		let mut raw = vec![7u8; 33];
		g1(sk).serialize_compressed(&mut raw).unwrap();
		g2(sk).serialize_compressed(&mut raw).unwrap();
		raw
	}

	/// The relay `Beefy::NextAuthorities` of the test validator set.
	fn next_authorities() -> Vec<BeefyId> {
		(1..=VALIDATORS)
			.map(|v| BeefyId::from_slice(&paired_key_bytes(Fr::from(v))).unwrap())
			.collect()
	}

	/// A fresh (randomized) single-dealer transcript by validator `dealer`,
	/// dealt to all validators with the worker's BFT parameters.
	fn dealing(dealer: u64) -> Vec<u8> {
		static SEED: AtomicU64 = AtomicU64::new(0);
		let mut rng = StdRng::seed_from_u64(SEED.fetch_add(1, Ordering::Relaxed));

		let validators = (1..=VALIDATORS).map(Fr::from).collect::<Vec<_>>();
		let n = validators.len();
		let f = (n - 1) / 3;
		let dkg = Dkg::<Bls12_381>::new(
			validators.iter().copied().map(g2).collect(),
			2 * f + 1,
			validators.iter().copied().map(g1).collect(),
			f + 1,
		)
		.unwrap();
		let sk = Fr::from(dealer);
		let transcript = dkg.deal_and_sign(&mut rng, (sk, g1(sk))).unwrap();

		let mut bytes = Vec::new();
		transcript.serialize_compressed(&mut bytes).unwrap();
		bytes
	}

	fn submit(session: SessionIndex, transcript_bytes: Vec<u8>) -> sp_runtime::DispatchResult {
		KeyGen::submit_dealing(RuntimeOrigin::none(), session, transcript_bytes)
	}

	fn add(dealer: u64, session: SessionIndex) -> sp_runtime::DispatchResult {
		submit(session, dealing(dealer))
	}

	/// The relay chain enters `session`; the dealer set arrives with the
	/// following block's relay state proof, as with the real collator.
	fn relay_session(session: SessionIndex) {
		KeyGen::on_relay_session(session, None);
		KeyGen::on_relay_session(session, Some(next_authorities()));
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext = sp_io::TestExternalities::new(storage);
		ext.execute_with(|| {
			System::set_block_number(1);
			relay_session(0);
		});
		ext
	}

	#[test]
	fn dealing_threshold_is_f_plus_one() {
		assert_eq!([1, 2, 3, 4, 7, 10, 100].map(dealing_threshold), [1, 1, 1, 2, 3, 4, 34]);
	}

	#[test]
	fn dealer_set_is_requested_until_the_round_starts() {
		new_test_ext().execute_with(|| {
			assert_eq!(relay_keys_to_prove::<Test>(), vec![relay_session_index_key()]);

			// A new relay session ends the round; the next one needs the dealer set.
			KeyGen::on_relay_session(1, None);
			assert!(CurrentRound::<Test>::get().is_none());
			assert_eq!(
				relay_keys_to_prove::<Test>(),
				vec![relay_session_index_key(), relay_next_authorities_key()]
			);

			KeyGen::on_relay_session(1, Some(next_authorities()));
			assert_eq!(CurrentRound::<Test>::get().map(|round| round.session), Some(1));
			assert_eq!(relay_keys_to_prove::<Test>(), vec![relay_session_index_key()]);
		});
	}

	#[test]
	fn round_ends_with_threshold_key_for_next_validator_set() {
		new_test_ext().execute_with(|| {
			System::assert_has_event(
				Event::<Test>::RoundStarted { session: 0, participants: 4 }.into(),
			);
			assert_ok!(add(1, 0));
			assert_ok!(add(2, 0));

			relay_session(1);

			// Round 0 dealt to the validators of relay session 1.
			let key = ThresholdKeys::<Test>::get(1).expect("2 of 4 dealers suffice");
			assert_eq!(key.dealers, 2);
			let secret_sharing =
				SecretSharing::<Bls12_381>::deserialize_compressed(&key.secret_sharing[..])
					.unwrap();
			assert_eq!(secret_sharing.bgpk.len(), VALIDATORS as usize);
			let _vk = ThresholdVk::from_share(&secret_sharing);
			let mut c = Vec::new();
			secret_sharing.c.serialize_compressed(&mut c).unwrap();
			System::assert_has_event(
				Event::<Test>::KeyGenerated { session: 1, dealers: 2, c }.into(),
			);

			// A fresh round started for the next queued set.
			assert!(AggregatedTranscript::<Test>::get().is_none());
			assert_eq!(CurrentRound::<Test>::get().map(|round| round.session), Some(1));
		});
	}

	#[test]
	fn rounds_are_session_scoped() {
		new_test_ext().execute_with(|| {
			assert_ok!(add(1, 0));
			assert_ok!(add(2, 0));
			relay_session(1);

			// Dealing again in the next session is a fresh round, not a duplicate;
			// a single dealer is below the threshold.
			assert_ok!(add(1, 1));
			relay_session(2);

			assert!(ThresholdKeys::<Test>::get(2).is_none());
			System::assert_has_event(
				Event::<Test>::KeyGenerationFailed { session: 2, dealers: 1, threshold: 2 }.into(),
			);
			assert_eq!(ThresholdKeys::<Test>::get(1).map(|key| key.dealers), Some(2));
		});
	}

	#[test]
	fn rejects_stale_foreign_and_duplicate_dealings() {
		new_test_ext().execute_with(|| {
			assert_noop!(add(1, 1), Error::<Test>::WrongSession);
			assert_noop!(add(9, 0), Error::<Test>::UnknownDealer);
			assert_noop!(submit(0, vec![1, 2, 3]), Error::<Test>::DeserializationFailed);
			assert_noop!(
				KeyGen::submit_dealing(RuntimeOrigin::signed(1), 0, dealing(1)),
				sp_runtime::DispatchError::BadOrigin
			);

			// At most one dealing per validator and round, even a fresh one.
			assert_ok!(add(1, 0));
			assert_noop!(add(1, 0), Error::<Test>::DuplicateDealer);
		});
	}

	#[test]
	#[allow(deprecated)]
	fn unsigned_validation_accepts_external_dealings_once_per_dealer() {
		new_test_ext().execute_with(|| {
			let call = |dealer, session| Call::<Test>::submit_dealing {
				session_index: session,
				transcript_bytes: dealing(dealer),
			};
			let validate = |call| KeyGen::validate_unsigned(TransactionSource::External, &call);

			let valid = validate(call(1, 0)).expect("a validator's dealing is valid");
			assert_eq!(valid.provides, vec![("KeyGenDealing", (0u32, 0u32)).encode()]);

			assert_eq!(validate(call(1, 1)), Err(InvalidTransaction::Future.into()));
			assert_eq!(validate(call(9, 0)), Err(InvalidTransaction::BadSigner.into()));
			assert_ok!(add(1, 0));
			assert_eq!(validate(call(1, 0)), Err(InvalidTransaction::Stale.into()));
		});
	}

	#[test]
	fn reads_the_relay_session_and_dealer_set_from_the_relay_state_proof() {
		use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

		new_test_ext().execute_with(|| {
			let proof = |session: SessionIndex, with_dealers: bool| {
				let mut builder = RelayStateSproofBuilder::default();
				builder
					.additional_key_values
					.push((relay_session_index_key(), session.encode()));
				if with_dealers {
					builder
						.additional_key_values
						.push((relay_next_authorities_key(), next_authorities().encode()));
				}
				let para_id = builder.para_id;
				let (root, proof) = builder.into_state_root_and_proof();
				RelayChainStateProof::new(para_id, root, proof).expect("valid relay proof")
			};

			assert_ok!(add(1, 0));
			assert_ok!(add(2, 0));
			KeyGen::on_relay_state_proof(&proof(1, false));
			assert!(ThresholdKeys::<Test>::contains_key(1));
			assert!(CurrentRound::<Test>::get().is_none());

			KeyGen::on_relay_state_proof(&proof(1, true));
			assert_eq!(CurrentRound::<Test>::get().map(|round| round.dealers.len()), Some(4));
		});
	}

	#[test]
	fn stored_aggregate_is_independent_of_submission_order() {
		let dealings = (1..=VALIDATORS).map(dealing).collect::<Vec<_>>();
		let aggregate_after = |order: &[usize]| {
			new_test_ext().execute_with(|| {
				for &i in order {
					assert_ok!(submit(0, dealings[i].clone()));
				}
				AggregatedTranscript::<Test>::get().expect("dealings were aggregated")
			})
		};

		// The receipts come out of `Dkg::aggregate` in hash-map order; the
		// pallet canonicalizes them, so block authors and importers agree.
		let reference = aggregate_after(&[0, 1, 2, 3]);
		for order in [[3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
			assert_eq!(aggregate_after(&order), reference);
		}
	}

	#[test]
	fn prunes_keys_beyond_history_depth() {
		new_test_ext().execute_with(|| {
			for session in 0..4 {
				assert_ok!(add(1, session));
				assert_ok!(add(2, session));
				relay_session(session + 1);
			}

			// `KeyHistoryDepth = 2`: only the keys of sessions 3 and 4 are kept.
			let kept = (1..=4).map(|s| ThresholdKeys::<Test>::contains_key(s)).collect::<Vec<_>>();
			assert_eq!(kept, [false, false, true, true]);
		});
	}

	#[test]
	fn session_key_g1_extracts_dealer_key() {
		let sk = Fr::from(42u64);
		let raw = paired_key_bytes(sk);
		assert_eq!(raw.len(), 177);

		let mut dealer_bytes = Vec::new();
		g1(sk).serialize_compressed(&mut dealer_bytes).unwrap();

		// The G1 component of the BEEFY paired key is exactly the a-DKG dealer key.
		assert_eq!(session_key_g1(&raw).unwrap(), &dealer_bytes[..]);
	}

	#[test]
	fn session_key_g1_rejects_wrong_length_keys() {
		assert!(session_key_g1(&[0u8; 176]).is_none());
		assert!(session_key_g1(&[0u8; 178]).is_none());
		assert!(session_key_g1(&[]).is_none());
	}
}
