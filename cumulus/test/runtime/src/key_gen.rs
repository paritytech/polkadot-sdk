//! On-chain aggregation of a-DKG dealings into per-session threshold keys.
//!
//! Every session is one DKG round: during session `s` the validators queued in
//! it (the validator set of session `s + 1`) submit their dealings, which are
//! verified and merged into the round's aggregate. The pallet is the session
//! handler of the BEEFY session key the DKG reuses, so at the rotation to
//! `s + 1` it ends the round — storing the threshold key under
//! [`ThresholdKeys`]`[s + 1]` if enough validators dealt — and starts a fresh
//! round for the newly queued set.

pub use pallet::*;

use alloc::vec::Vec;

use frame_support::traits::{Get, OneSessionHandler};
use sp_runtime::{BoundToRuntimeAppPublic, SaturatedConversion};

/// Index of a session (same as `sp_staking::SessionIndex`).
pub type SessionIndex = u32;

impl<T: Config> BoundToRuntimeAppPublic for Pallet<T> {
	type Public = sp_consensus_beefy::ecdsa_bls_crypto::AuthorityId;
}

/// Session handler for the BEEFY session key in runtimes without `pallet_beefy`.
///
/// The DKG reuses the validators' BEEFY (ECDSA,BLS12-381) session keys
/// (decision: Alistair, 2026-09-29). Validator keys are looked up through
/// [`Config::DkgKeyOf`]; the handler only drives the round lifecycle.
impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
	type Key = sp_consensus_beefy::ecdsa_bls_crypto::AuthorityId;

	fn on_genesis_session<'a, I: 'a>(validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
		T::AccountId: 'a,
	{
		// The genesis set is also the set queued for the second session.
		Self::start_round(T::CurrentSessionIndex::get(), validators.count().saturated_into());
	}

	fn on_new_session<'a, I: 'a>(_changed: bool, _validators: I, queued_validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
		T::AccountId: 'a,
	{
		// `pallet_session` bumps the session index before notifying handlers.
		Self::end_round();
		Self::start_round(
			T::CurrentSessionIndex::get(),
			queued_validators.count().saturated_into(),
		);
	}

	fn on_disabled(_: u32) {}
}

/// Look up an account's registered BEEFY session key.
///
/// Implemented by the runtime; backed by the session pallet's queued keys when the
/// runtime has one. The `()` impl models "no session pallet": nobody is a validator.
pub trait DkgKeyLookup<AccountId> {
	/// The account's registered BEEFY session key, or `None` if `who` is not a
	/// (queued) validator or has no BEEFY key registered.
	fn dkg_key(who: &AccountId) -> Option<Vec<u8>>;
}

impl<AccountId> DkgKeyLookup<AccountId> for () {
	fn dkg_key(_: &AccountId) -> Option<Vec<u8>> {
		None
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
	use alloc::vec::Vec;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::SaturatedConversion;

	use adkg_vrf::dkg::{transcript::Transcript, Dkg};
	use ark_bls12_381::Bls12_381;
	use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

	use super::{dealing_threshold, session_key_g1, DkgKeyLookup, SessionIndex};

	type DkgTranscript = Transcript<Bls12_381>;

	/// The DKG round in progress.
	///
	/// Round `session` collects the dealings submitted during that session for
	/// the validator set queued in it, i.e. the validators of session
	/// `session + 1`.
	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct Round {
		/// The session in which the round's dealings are submitted.
		pub session: SessionIndex,
		/// Size of the validator set being dealt to.
		pub participants: u32,
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

		/// Look up the registered BEEFY session key of a validator account, if any.
		type DkgKeyOf: DkgKeyLookup<Self::AccountId>;

		/// The current session index (e.g. `pallet_session::CurrentIndex`).
		type CurrentSessionIndex: Get<SessionIndex>;

		/// Number of most recent sessions whose threshold keys are kept in
		/// [`ThresholdKeys`].
		#[pallet::constant]
		type KeyHistoryDepth: Get<SessionIndex>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A valid dealing was merged into the aggregate of round `session`.
		TranscriptAdded { who: T::AccountId, session: SessionIndex },
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
		/// No DKG round is in progress (the session handler has not run yet).
		NoRound,
		/// The dealing was made for another session than the current round's.
		WrongSession,
		/// The origin is not a queued validator with a registered BEEFY session key.
		NotAValidator,
		/// The transcript has no contribution receipts.
		NoDealers,
		/// The transcript must come from a single dealer, dealing with the
		/// submitter's own BEEFY session key.
		NotSingleDealer,
		/// The transcript's dealer key does not match the submitter's BEEFY session key.
		UnknownDealer,
		/// The submitter already dealt in the current round.
		DuplicateDealer,
	}

	/// The DKG round in progress.
	#[pallet::storage]
	pub type CurrentRound<T: Config> = StorageValue<_, Round, OptionQuery>;

	/// Aggregate of the current round's dealings; reset at every session rotation.
	#[pallet::storage]
	#[pallet::getter(fn aggregated_transcript)]
	pub type AggregatedTranscript<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Threshold keys by the session whose validator set holds them, for the
	/// last [`Config::KeyHistoryDepth`] sessions.
	#[pallet::storage]
	pub type ThresholdKeys<T: Config> =
		StorageMap<_, Twox64Concat, SessionIndex, ThresholdKey, OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a dealing for the current round; it is verified and merged into
		/// the round's aggregate.
		///
		/// `session_index` is the session the dealing was made in and must be the
		/// current round's: a dealing is made for the validator set queued in its
		/// session, so one landing after the next rotation is stale.
		///
		/// Only queued validators can submit, once per round, and the transcript
		/// must be a fresh (single-dealer) dealing created with the submitter's own
		/// BEEFY session key: the `ContributionReceipt` proof of possession binds
		/// the dealer key, which must equal the G1 component of the submitter's
		/// BEEFY paired key. Aggregation of multiple dealings happens on-chain via
		/// this pallet's own merge logic.
		#[pallet::call_index(0)]
		#[pallet::weight(10_000_000)] // TODO: benchmark
		pub fn add_transcript(
			origin: OriginFor<T>,
			session_index: SessionIndex,
			transcript_bytes: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let round = CurrentRound::<T>::get().ok_or(Error::<T>::NoRound)?;
			ensure!(session_index == round.session, Error::<T>::WrongSession);

			let session_key = T::DkgKeyOf::dkg_key(&who).ok_or(Error::<T>::NotAValidator)?;
			let session_g1 = session_key_g1(&session_key).ok_or(Error::<T>::NotAValidator)?;

			let new_transcript = DkgTranscript::deserialize_compressed(&transcript_bytes[..])
				.map_err(|_| Error::<T>::DeserializationFailed)?;

			// TODO: full PVSS verification needs deterministic Fiat-Shamir challenges
			// (adkg-vrf `verify_with_fs`); until then only the deterministic checks run.
			for (receipt, _w) in &new_transcript.receipts {
				receipt.verify_all_sigs().map_err(|_| Error::<T>::VerificationFailed)?;
			}
			new_transcript.check_consistency().map_err(|_| Error::<T>::VerificationFailed)?;

			// Bind the dealing to the submitter: it must be a fresh, single-dealer
			// transcript whose dealer key is the G1 component of the submitter's
			// BEEFY paired session key.
			let dealers = new_transcript.list_dealers();
			let dealer_pk = dealers.first().ok_or(Error::<T>::NoDealers)?;
			ensure!(dealers.iter().all(|d| d == dealer_pk), Error::<T>::NotSingleDealer);
			let mut dealer_bytes = Vec::new();
			dealer_pk
				.serialize_compressed(&mut dealer_bytes)
				.map_err(|_| Error::<T>::SerializationFailed)?;
			ensure!(dealer_bytes == session_g1, Error::<T>::UnknownDealer);

			let mut aggregated = if let Some(existing_bytes) = AggregatedTranscript::<T>::get() {
				let existing = DkgTranscript::deserialize_compressed(&existing_bytes[..])
					.map_err(|_| Error::<T>::DeserializationFailed)?;
				// A second dealing would count the dealer's weight twice.
				ensure!(!existing.list_dealers().contains(dealer_pk), Error::<T>::DuplicateDealer);
				Dkg::<Bls12_381>::aggregate(alloc::vec![existing, new_transcript])
			} else {
				new_transcript
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

			Self::deposit_event(Event::TranscriptAdded { who, session: session_index });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Starts round `session`, dealing to `participants` validators.
		pub(crate) fn start_round(session: SessionIndex, participants: u32) {
			AggregatedTranscript::<T>::kill();
			CurrentRound::<T>::put(Round { session, participants });
		}

		/// Ends the current round: stores its threshold key if enough validators
		/// dealt, and prunes keys older than [`Config::KeyHistoryDepth`] sessions.
		pub(crate) fn end_round() {
			let Some(round) = CurrentRound::<T>::take() else { return };
			let session = round.session.saturating_add(1);
			let threshold = dealing_threshold(round.participants);

			// The aggregate is written by `add_transcript` from validated
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
	use frame_support::{assert_noop, assert_ok, derive_impl, parameter_types, traits::ConstU32};
	use sp_core::crypto::ByteArray;
	use sp_runtime::BuildStorage;

	type AuthorityId = sp_consensus_beefy::ecdsa_bls_crypto::AuthorityId;

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

	parameter_types! {
		pub storage CurrentSession: SessionIndex = 0;
	}

	/// Accounts `1..=VALIDATORS` are the queued validators; account `v`'s BLS
	/// secret is `Fr::from(v)`. With `n = 4`, `f = 1` and 2 dealers are needed.
	const VALIDATORS: u64 = 4;

	pub struct TestDkgKeys;

	impl DkgKeyLookup<u64> for TestDkgKeys {
		fn dkg_key(who: &u64) -> Option<Vec<u8>> {
			(1..=VALIDATORS).contains(who).then(|| paired_key_bytes(Fr::from(*who)))
		}
	}

	impl Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type DkgKeyOf = TestDkgKeys;
		type CurrentSessionIndex = CurrentSession;
		type KeyHistoryDepth = ConstU32<2>;
	}

	fn g1(sk: Fr) -> G1Affine {
		(G1Projective::generator() * sk).into_affine()
	}

	fn g2(sk: Fr) -> G2Affine {
		(G2Projective::generator() * sk).into_affine()
	}

	/// Builds a serialized BEEFY paired key `[ecdsa (33B) || pk_G1 (48B) || pk_G2 (96B)]`
	/// for the given BLS secret, as stored in the session key store. The ECDSA
	/// component is opaque filler (irrelevant to the DKG).
	fn paired_key_bytes(sk: Fr) -> Vec<u8> {
		let mut raw = vec![7u8; 33];
		g1(sk).serialize_compressed(&mut raw).unwrap();
		g2(sk).serialize_compressed(&mut raw).unwrap();
		raw
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

	fn add(who: u64, session: SessionIndex) -> frame_support::dispatch::DispatchResult {
		KeyGen::add_transcript(RuntimeOrigin::signed(who), session, dealing(who))
	}

	fn validator_keys() -> Vec<(u64, AuthorityId)> {
		(1..=VALIDATORS)
			.map(|v| (v, AuthorityId::from_slice(&paired_key_bytes(Fr::from(v))).unwrap()))
			.collect()
	}

	fn keyed(keys: &[(u64, AuthorityId)]) -> impl Iterator<Item = (&u64, AuthorityId)> {
		keys.iter().map(|(who, key)| (who, key.clone()))
	}

	/// Rotates to `session` with an unchanged validator set; like `pallet_session`,
	/// the session index is bumped before the handlers run.
	fn rotate_to(session: SessionIndex) {
		CurrentSession::set(&session);
		let keys = validator_keys();
		<KeyGen as OneSessionHandler<u64>>::on_new_session(true, keyed(&keys), keyed(&keys));
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext = sp_io::TestExternalities::new(storage);
		ext.execute_with(|| {
			System::set_block_number(1);
			let keys = validator_keys();
			<KeyGen as OneSessionHandler<u64>>::on_genesis_session(keyed(&keys));
		});
		ext
	}

	#[test]
	fn dealing_threshold_is_f_plus_one() {
		assert_eq!([1, 2, 3, 4, 7, 10, 100].map(dealing_threshold), [1, 1, 1, 2, 3, 4, 34]);
	}

	#[test]
	fn round_ends_with_threshold_key_for_next_validator_set() {
		new_test_ext().execute_with(|| {
			assert_eq!(CurrentRound::<Test>::get(), Some(Round { session: 0, participants: 4 }));
			assert_ok!(add(1, 0));
			assert_ok!(add(2, 0));

			rotate_to(1);

			// Round 0 dealt to the validators of session 1.
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
			assert_eq!(CurrentRound::<Test>::get(), Some(Round { session: 1, participants: 4 }));
		});
	}

	#[test]
	fn rounds_are_session_scoped() {
		new_test_ext().execute_with(|| {
			assert_ok!(add(1, 0));
			assert_ok!(add(2, 0));
			rotate_to(1);

			// Dealing again in the next session is a fresh round, not a duplicate;
			// a single dealer is below the threshold.
			assert_ok!(add(1, 1));
			rotate_to(2);

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
			assert_noop!(
				KeyGen::add_transcript(RuntimeOrigin::signed(9), 0, dealing(1)),
				Error::<Test>::NotAValidator
			);
			assert_noop!(
				KeyGen::add_transcript(RuntimeOrigin::signed(2), 0, dealing(1)),
				Error::<Test>::UnknownDealer
			);
			assert_noop!(
				KeyGen::add_transcript(RuntimeOrigin::signed(1), 0, vec![1, 2, 3]),
				Error::<Test>::DeserializationFailed
			);

			// At most one dealing per validator and round, even a fresh one.
			assert_ok!(add(1, 0));
			assert_noop!(add(1, 0), Error::<Test>::DuplicateDealer);
		});
	}

	#[test]
	fn stored_aggregate_is_independent_of_submission_order() {
		let dealings = (1..=VALIDATORS).map(|v| (v, dealing(v))).collect::<Vec<_>>();
		let aggregate_after = |order: &[usize]| {
			new_test_ext().execute_with(|| {
				for &i in order {
					let (who, transcript) = dealings[i].clone();
					assert_ok!(KeyGen::add_transcript(RuntimeOrigin::signed(who), 0, transcript));
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
				rotate_to(session + 1);
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
