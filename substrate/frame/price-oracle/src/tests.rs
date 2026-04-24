use super::*;
use crate as pallet_price_oracle;

use alloc::vec;
use frame_support::{assert_noop, assert_ok, derive_impl, parameter_types};
use pallet_price_oracle::ParsingMethod;
use sp_consensus_babe::{AuthorityId, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_core::{crypto::Pair as PairT, sr25519};
use sp_inherents::InherentData;
use sp_io::TestExternalities;
use sp_price_oracle::{
	Nudge, PairConfig, PairId, PriceOracleInherentData, SignedNudge, INHERENT_IDENTIFIER,
};
use sp_runtime::{traits::BadOrigin, BuildStorage, FixedU128};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		PriceOracle: pallet_price_oracle,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

pub struct MockTime;
impl frame_support::traits::Time for MockTime {
	type Moment = u64;
	fn now() -> u64 {
		0
	}
}

parameter_types! {
	pub const MaxEndpoints: u32 = 20;
	pub const MaxUrlLength: u32 = 64;
}

thread_local! {
	static AUTHORITIES: std::cell::RefCell<Vec<AuthorityId>> = std::cell::RefCell::new(Vec::new());
	static CURRENT_SLOT: std::cell::RefCell<Slot> = std::cell::RefCell::new(Slot::from(1u64));
}

pub struct MockAuthorityProvider;
impl pallet::AuthorityProvider for MockAuthorityProvider {
	fn authorities() -> Vec<AuthorityId> {
		AUTHORITIES.with(|a| a.borrow().clone())
	}
	fn current_slot() -> Slot {
		CURRENT_SLOT.with(|s| *s.borrow())
	}
}

fn set_authorities(pairs: &[sr25519::Pair]) {
	let authorities: Vec<AuthorityId> =
		pairs.iter().map(|p| AuthorityId::from(p.public())).collect();
	AUTHORITIES.with(|a| *a.borrow_mut() = authorities);
}

fn set_current_slot(slot: u64) {
	CURRENT_SLOT.with(|s| *s.borrow_mut() = Slot::from(slot));
}

impl Config for Test {
	type AuthorityProvider = MockAuthorityProvider;
	type TimeProvider = MockTime;
	type OnPriceUpdate = ();
	type MaxEndpoints = MaxEndpoints;
	type MaxUrlLength = MaxUrlLength;
	type PriceOracleOrigin = frame_system::EnsureRoot<u64>;
}

/// Default config values used by the builder when no explicit pair setup is provided.
fn default_pair_config() -> PairConfig {
	PairConfig {
		min_nudges: 0,
		nudge_validity: 10,
		inherent_mandatory: false,
		invalid_inherent_panics: false,
		epsilon: FixedU128::from_rational(1, 100),
	}
}

fn cfg_with(
	min_nudges: u32,
	nudge_validity: u64,
	inherent_mandatory: bool,
	invalid_inherent_panics: bool,
) -> PairConfig {
	PairConfig {
		min_nudges,
		nudge_validity,
		inherent_mandatory,
		invalid_inherent_panics,
		epsilon: FixedU128::from_rational(1, 100),
	}
}

struct ExtBuilder {
	num_authorities: usize,
	current_slot: u64,
	pairs: Vec<(PairId, PairConfig, FixedU128)>, // (id, config, initial_price)
}

impl Default for ExtBuilder {
	fn default() -> Self {
		set_authorities(&generate_test_pairs(3));
		Self {
			num_authorities: 3,
			current_slot: 5,
			pairs: vec![(0u8, default_pair_config(), FixedU128::zero())],
		}
	}
}

impl ExtBuilder {
	fn authorities(mut self, n: usize) -> Self {
		self.num_authorities = n;
		self
	}

	fn current_slot(mut self, slot: u64) -> Self {
		self.current_slot = slot;
		self
	}

	fn pair(mut self, id: PairId, config: PairConfig) -> Self {
		// Replace the default pair 0 with the provided config.
		self.pairs.retain(|(pid, _, _)| *pid != id);
		self.pairs.push((id, config, FixedU128::zero()));
		self
	}

	fn pair_with_price(mut self, id: PairId, config: PairConfig, price: FixedU128) -> Self {
		self.pairs.retain(|(pid, _, _)| *pid != id);
		self.pairs.push((id, config, price));
		self
	}

	fn no_pairs(mut self) -> Self {
		self.pairs.clear();
		self
	}

	fn build(self) -> TestExternalities {
		let pairs = generate_test_pairs(self.num_authorities);
		set_authorities(&pairs);
		set_current_slot(self.current_slot);

		let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext = TestExternalities::new(t);

		ext.execute_with(|| {
			for (pair_id, cfg, initial_price) in &self.pairs {
				pallet::Pairs::<Test>::insert(pair_id, cfg.clone());
				if !initial_price.is_zero() {
					pallet::CurrentPrice::<Test>::insert(pair_id, *initial_price);
				}
			}
		});

		ext
	}

	fn build_and_execute(self, test: impl FnOnce() -> ()) {
		self.build().execute_with(|| {
			test();
		});
	}
}

fn make_signed_nudge(
	pair: &sr25519::Pair,
	nudge: Nudge,
	slot: u64,
	authority_index: u32,
) -> SignedNudge {
	let slot = Slot::from(slot);
	let payload = SignedNudge::signing_payload(&nudge, slot);
	let raw_sig = pair.sign(&payload);
	let signature = AuthoritySignature::from(raw_sig);
	SignedNudge { nudge, slot, authority_index, signature }
}

fn generate_test_pairs(count: usize) -> Vec<sr25519::Pair> {
	(0..count).map(|i| sr25519::Pair::from_seed(&[i as u8; 32])).collect()
}

#[test]
fn price_starts_at_zero() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(PriceOracle::current_price(0), FixedU128::zero());
	});
}

#[test]
fn single_up_nudge_increases_price_by_epsilon() {
	ExtBuilder::default().build_and_execute(|| {
		let pairs = generate_test_pairs(3);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![(0u8, vec![nudge])],
		));

		let expected = FixedU128::from_rational(1, 100);
		assert_eq!(PriceOracle::current_price(0), expected);
	});
}

#[test]
fn multiple_ups_compound() {
	ExtBuilder::default().build_and_execute(|| {
		let pairs = generate_test_pairs(3);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			make_signed_nudge(&pairs[2], Nudge::Up, 4, 2),
		];
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![(0u8, nudges)],
		));

		// 3 ups, 0 downs → net 3 → price = 3 * 0.01 = 0.03
		assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(3, 100));
	});
}

#[test]
fn ups_and_downs_cancel_out() {
	ExtBuilder::default().authorities(4).build_and_execute(|| {
		let pairs = generate_test_pairs(4);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			make_signed_nudge(&pairs[2], Nudge::Down, 4, 2),
			make_signed_nudge(&pairs[3], Nudge::Up, 4, 3),
		];
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![(0u8, nudges)],
		));

		// 3 ups, 1 down → net 2 up → price = 2 * 0.01 = 0.02
		assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(2, 100));
	});
}

#[test]
fn down_nudges_decrease_price() {
	ExtBuilder::default()
		.pair_with_price(0, default_pair_config(), FixedU128::from_u32(1))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(3);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Down, 5, 0),
				make_signed_nudge(&pairs[1], Nudge::Down, 5, 1),
			];
			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, nudges)],
			));

			// 0 ups, 2 downs → net 2 down → 1.0 - 0.02 = 0.98
			assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(98, 100));
		});
}

#[test]
fn stale_nudge_returns_error() {
	ExtBuilder::default().authorities(2).current_slot(15).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 15, 0), // valid
			make_signed_nudge(&pairs[1], Nudge::Up, 4, 1),  // stale (15 - 4 = 11 >= 10)
		];
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![(0u8, nudges)],),
			Error::<Test>::StaleNudge
		);
	});
}

#[test]
fn invalid_signature_returns_error() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		// Sign with pair[1]'s key but claim to be authority 0
		let bad_nudge = make_signed_nudge(&pairs[1], Nudge::Up, 5, 0);
		let good_nudge = make_signed_nudge(&pairs[1], Nudge::Up, 5, 1);

		assert_noop!(
			PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![bad_nudge, good_nudge])],
			),
			Error::<Test>::InvalidSignature
		);
	});
}
#[test]
fn invalid_signature_alone_returns_error() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		// Sign with pair[1]'s key but claim to be authority 0
		let bad_nudge = make_signed_nudge(&pairs[1], Nudge::Up, 5, 0);

		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![(0u8, vec![bad_nudge])]),
			Error::<Test>::InvalidSignature
		);
	});
}
#[test]
fn price_cannot_go_below_zero() {
	ExtBuilder::default().authorities(1).current_slot(5).build_and_execute(|| {
		let pairs = generate_test_pairs(1);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Down, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![(0u8, vec![nudge])],
		));
		assert_eq!(PriceOracle::current_price(0), FixedU128::zero());
	});
}

#[test]
fn empty_nudges_does_not_change_price() {
	ExtBuilder::default()
		.pair_with_price(0, default_pair_config(), FixedU128::from_u32(5))
		.build_and_execute(|| {
			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![])],
			));
			assert_eq!(PriceOracle::current_price(0), FixedU128::from_u32(5));
		});
}

#[test]
fn submit_nudges_only_once_per_block() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let n1 = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![(0u8, vec![n1])],
		));

		let n2 = make_signed_nudge(&pairs[1], Nudge::Up, 5, 1);
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![(0u8, vec![n2])],),
			Error::<Test>::DuplicateInherent
		);
	});
}

#[test]
fn duplicate_authority_nudges_return_error() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[0], Nudge::Up, 4, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
		];
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![(0u8, nudges)],),
			Error::<Test>::DuplicateNudge
		);
	});
}

#[test]
fn too_few_nudges_returns_error() {
	ExtBuilder::default()
		.pair(0, cfg_with(2, 10, false, false))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(3);
			let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_noop!(
				PriceOracle::submit_nudges(
					frame_system::RawOrigin::None.into(),
					vec![(0u8, vec![nudge])],
				),
				Error::<Test>::TooFewNudges,
			);
		});
}

#[test]
fn register_pair_works() {
	ExtBuilder::default().no_pairs().build_and_execute(|| {
		let cfg = cfg_with(1, 5, true, false);
		assert_ok!(PriceOracle::register_pair(
			frame_system::RawOrigin::Root.into(),
			7,
			cfg.clone(),
		));
		assert_eq!(pallet::Pairs::<Test>::get(7), Some(cfg));
	});
}

#[test]
fn register_pair_duplicate_rejected() {
	ExtBuilder::default().build_and_execute(|| {
		assert_noop!(
			PriceOracle::register_pair(
				frame_system::RawOrigin::Root.into(),
				0,
				default_pair_config(),
			),
			Error::<Test>::PairAlreadyExists,
		);
	});
}

#[test]
fn register_pair_requires_custom_origin() {
	ExtBuilder::default().no_pairs().build_and_execute(|| {
		assert_noop!(
			PriceOracle::register_pair(
				frame_system::RawOrigin::Signed(1).into(),
				0,
				default_pair_config(),
			),
			BadOrigin,
		);
	});
}

#[test]
fn update_pair_config_works() {
	ExtBuilder::default().build_and_execute(|| {
		let new_cfg = cfg_with(3, 20, true, true);
		assert_ok!(PriceOracle::update_pair_config(
			frame_system::RawOrigin::Root.into(),
			0,
			new_cfg.clone(),
		));
		assert_eq!(pallet::Pairs::<Test>::get(0), Some(new_cfg));
	});
}

#[test]
fn update_pair_config_unknown_rejected() {
	ExtBuilder::default().build_and_execute(|| {
		assert_noop!(
			PriceOracle::update_pair_config(
				frame_system::RawOrigin::Root.into(),
				5,
				default_pair_config(),
			),
			Error::<Test>::UnknownPair,
		);
	});
}

#[test]
fn remove_pair_clears_storage() {
	ExtBuilder::default().build_and_execute(|| {
		// Default builder registers pair 0; seed per-pair state expected to be cleared.
		pallet::CurrentPrice::<Test>::insert(0u8, FixedU128::from_u32(42));
		pallet::InherentSeen::<Test>::insert(0u8, true);

		assert_ok!(PriceOracle::remove_pair(frame_system::RawOrigin::Root.into(), 0));
		assert!(!pallet::Pairs::<Test>::contains_key(0u8));
		assert_eq!(pallet::CurrentPrice::<Test>::get(0u8), FixedU128::zero());
		assert!(!pallet::ActiveEndpoints::<Test>::contains_key(0u8));
		assert!(!pallet::InherentSeen::<Test>::contains_key(0u8));
	});
}

#[test]
fn remove_pair_unknown_rejected() {
	ExtBuilder::default().build_and_execute(|| {
		assert_noop!(
			PriceOracle::remove_pair(frame_system::RawOrigin::Root.into(), 5),
			Error::<Test>::UnknownPair,
		);
	});
}

#[test]
fn set_active_endpoints_requires_pair() {
	ExtBuilder::default().build_and_execute(|| {
		let endpoints = vec![(u8::from(ParsingMethod::Binance), b"https://x".to_vec())];
		assert_noop!(
			PriceOracle::set_active_endpoints(frame_system::RawOrigin::Root.into(), 5, endpoints,),
			Error::<Test>::UnknownPair,
		);
	});
}

#[test]
fn set_active_endpoints_stores_per_pair() {
	ExtBuilder::default().build_and_execute(|| {
		let endpoints = vec![
			(u8::from(ParsingMethod::Binance), b"https://binance.example/price".to_vec()),
			(u8::from(ParsingMethod::CoinGecko), b"https://coingecko.example/price".to_vec()),
		];
		assert_ok!(PriceOracle::set_active_endpoints(
			frame_system::RawOrigin::Root.into(),
			0,
			endpoints,
		));
		let stored: Vec<(u8, Vec<u8>)> = pallet::ActiveEndpoints::<Test>::get(0u8)
			.into_iter()
			.map(|(m, url)| (m.into(), url.into_inner()))
			.collect();
		assert_eq!(stored.len(), 2);
	});
}

#[test]
fn set_active_endpoints_rejects_too_many() {
	ExtBuilder::default().build_and_execute(|| {
		let endpoints: Vec<_> =
			(0..21).map(|_| (u8::from(ParsingMethod::Binance), b"x".to_vec())).collect();
		assert_noop!(
			PriceOracle::set_active_endpoints(frame_system::RawOrigin::Root.into(), 0, endpoints,),
			Error::<Test>::TooManyEndpoints,
		);
	});
}

#[test]
fn set_active_endpoints_rejects_url_too_long() {
	ExtBuilder::default().build_and_execute(|| {
		let long_url = vec![b'x'; 65];
		assert_noop!(
			PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				0,
				vec![(u8::from(ParsingMethod::Binance), long_url)],
			),
			Error::<Test>::UrlTooLong,
		);
	});
}

#[test]
fn set_active_endpoints_rejects_unknown_method() {
	ExtBuilder::default().build_and_execute(|| {
		assert_noop!(
			PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				0,
				vec![(99u8, b"https://x".to_vec())],
			),
			Error::<Test>::UnknownParsingMethod,
		);
	});
}

// ----- Multi-pair inherent behaviour ----------------------------------------

#[test]
fn multi_pair_inherent_updates_both_prices() {
	ExtBuilder::default()
		.authorities(3)
		.pair(0, default_pair_config())
		.pair(1, default_pair_config())
		.build_and_execute(|| {
			let pairs = generate_test_pairs(3);
			let nudges_a = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			];
			let nudges_b = vec![
				make_signed_nudge(&pairs[0], Nudge::Down, 5, 0),
				make_signed_nudge(&pairs[2], Nudge::Down, 5, 2),
			];

			// Seed prices so Down can move.
			pallet::CurrentPrice::<Test>::insert(1u8, FixedU128::from_u32(1));

			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, nudges_a), (1u8, nudges_b)],
			));

			// Pair 0: 2 ups → +0.02
			assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(2, 100));
			// Pair 1: 2 downs → 1.0 - 0.02 = 0.98
			assert_eq!(PriceOracle::current_price(1), FixedU128::from_rational(98, 100));
		});
}

#[test]
fn duplicate_pair_in_inherent_rejected() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_noop!(
			PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![nudge.clone()]), (0u8, vec![nudge])],
			),
			Error::<Test>::DuplicatePairInInherent,
		);
	});
}

#[test]
fn unknown_pair_in_inherent_rejected() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_noop!(
			PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(9u8, vec![nudge])],
			),
			Error::<Test>::UnknownPair,
		);
	});
}

#[test]
fn unknown_pair_never_panics_even_when_other_pair_panics() {
	// Pair 0 has invalid_inherent_panics=true, but an unknown pair should always return an
	// error, never panic. To assert no panic, run in a normal (non-should_panic) test.
	ExtBuilder::default()
		.authorities(2)
		.pair(0, cfg_with(0, 10, false, true))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_noop!(
				PriceOracle::submit_nudges(
					frame_system::RawOrigin::None.into(),
					vec![(9u8, vec![nudge])],
				),
				Error::<Test>::UnknownPair,
			);
		});
}

#[test]
#[should_panic(expected = "inherent_mandatory")]
fn per_pair_inherent_mandatory_panics_in_finalize() {
	ExtBuilder::default()
		.pair(0, cfg_with(0, 10, true, false))
		.build_and_execute(|| {
			PriceOracle::on_finalize(1);
		});
}

#[test]
fn per_pair_inherent_mandatory_not_set_does_not_panic() {
	ExtBuilder::default()
		.pair(0, cfg_with(0, 10, false, false))
		.build_and_execute(|| {
			PriceOracle::on_finalize(1);
		});
}

#[test]
fn mandatory_pair_with_inherent_seen_does_not_panic() {
	ExtBuilder::default()
		.pair(0, cfg_with(0, 10, true, false))
		.build_and_execute(|| {
			pallet::InherentSeen::<Test>::insert(0u8, true);
			PriceOracle::on_finalize(1);
		});
}

#[test]
fn invalid_inherent_panics_converts_error_to_panic() {
	ExtBuilder::default()
		.authorities(2)
		.current_slot(20)
		.pair(0, cfg_with(0, 10, false, true)) // invalid_inherent_panics = true
		.build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let stale_nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_noop!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![stale_nudge])],
			), Error::<Test>::StaleNudge);
		});
}

#[test]
fn invalid_inherent_panics_false_returns_error() {
	ExtBuilder::default()
		.authorities(2)
		.current_slot(20)
		.pair(0, cfg_with(0, 10, false, false))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let stale_nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_noop!(
				PriceOracle::submit_nudges(
					frame_system::RawOrigin::None.into(),
					vec![(0u8, vec![stale_nudge])],
				),
				Error::<Test>::StaleNudge,
			);
		});
}

#[test]
fn partial_inherent_with_mandatory_mix_ok() {
	// Pair 0 mandatory, pair 1 not. Inherent includes only pair 0 → OK.
	ExtBuilder::default()
		.authorities(1)
		.pair(0, cfg_with(0, 10, true, false))
		.pair(1, cfg_with(0, 10, false, false))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(1);
			let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![nudge])],
			));
			PriceOracle::on_finalize(1);
		});
}

#[test]
#[should_panic(expected = "inherent_mandatory")]
fn partial_inherent_missing_mandatory_panics() {
	// Pair 1 mandatory, inherent only contains pair 0 → on_finalize panics.
	ExtBuilder::default()
		.authorities(1)
		.pair(0, cfg_with(0, 10, false, false))
		.pair(1, cfg_with(0, 10, true, false))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(1);
			let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![nudge])],
			));
			PriceOracle::on_finalize(1);
		});
}

#[test]
fn nudges_are_isolated_per_pair() {
	// Same authority can submit a nudge for pair 0 AND pair 1 in the same inherent.
	ExtBuilder::default()
		.authorities(1)
		.pair(0, default_pair_config())
		.pair(1, default_pair_config())
		.build_and_execute(|| {
			let pairs = generate_test_pairs(1);
			let n0 = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			let n1 = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
			assert_ok!(PriceOracle::submit_nudges(
				frame_system::RawOrigin::None.into(),
				vec![(0u8, vec![n0]), (1u8, vec![n1])],
			));
			assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(1, 100));
			assert_eq!(PriceOracle::current_price(1), FixedU128::from_rational(1, 100));
		});
}

mod inherent_pipeline {
	use super::*;
	use frame_support::{pallet_prelude::ProvideInherent, traits::UnfilteredDispatchable};

	fn build_inherent_data(pair_nudges: Vec<(PairId, Vec<SignedNudge>)>) -> InherentData {
		let mut data = InherentData::new();
		let payload: PriceOracleInherentData = pair_nudges;
		data.put_data(INHERENT_IDENTIFIER, &payload).expect("puts inherent data");
		data
	}

	fn run_inherent(pair_nudges: Vec<(PairId, Vec<SignedNudge>)>) {
		let data = build_inherent_data(pair_nudges);
		let call = PriceOracle::create_inherent(&data).expect("create_inherent returns Some");
		assert_ok!(call.dispatch_bypass_filter(frame_system::RawOrigin::None.into()));
	}

	fn run_inherent_expect_err(
		pair_nudges: Vec<(PairId, Vec<SignedNudge>)>,
		expected: Error<Test>,
	) {
		let data = build_inherent_data(pair_nudges);
		let call = PriceOracle::create_inherent(&data).expect("create_inherent returns Some");
		let result = call.dispatch_bypass_filter(frame_system::RawOrigin::None.into());
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, expected.into());
	}

	#[test]
	fn happy_path() {
		ExtBuilder::default().authorities(3).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(3);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 9, 1),
				make_signed_nudge(&pairs[2], Nudge::Down, 10, 2),
			];
			run_inherent(vec![(0u8, nudges)]);

			assert_eq!(PriceOracle::current_price(0), FixedU128::from_rational(1, 100));
		});
	}

	#[test]
	fn duplicates_return_error() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[0], Nudge::Up, 9, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent_expect_err(vec![(0u8, nudges)], Error::<Test>::DuplicateNudge);
			assert_eq!(PriceOracle::current_price(0), FixedU128::zero());
		});
	}

	#[test]
	fn stale_nudge_returns_error() {
		ExtBuilder::default().authorities(2).current_slot(20).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 20, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			];
			run_inherent_expect_err(vec![(0u8, nudges)], Error::<Test>::StaleNudge);
		});
	}

	#[test]
	fn bad_signature_returns_error() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent_expect_err(vec![(0u8, nudges)], Error::<Test>::InvalidSignature);
		});
	}

	#[test]
	fn empty_inherent() {
		ExtBuilder::default()
			.pair_with_price(0, default_pair_config(), FixedU128::from_u32(5))
			.build_and_execute(|| {
				run_inherent(vec![]);
				assert_eq!(PriceOracle::current_price(0), FixedU128::from_u32(5));
			});
	}

	#[test]
	fn invalid_authority_is_rejected() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![make_signed_nudge(&pairs[1], Nudge::Up, 10, 3)];
			run_inherent_expect_err(vec![(0u8, nudges)], Error::<Test>::InvalidAuthority);
		});
	}
}

mod active_endpoints {
	use super::*;
	use sp_runtime::DispatchError;

	fn stored_ids(pair_id: PairId) -> Vec<(u8, Vec<u8>)> {
		pallet::ActiveEndpoints::<Test>::get(pair_id)
			.into_iter()
			.map(|(m, url)| (m.into(), url.into_inner()))
			.collect()
	}

	#[test]
	fn starts_empty() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(pallet::ActiveEndpoints::<Test>::get(0u8).is_empty());
			assert!(pallet::ActiveEndpoints::<Test>::get(1u8).is_empty());
		});
	}

	#[test]
	fn custom_origin_can_set() {
		ExtBuilder::default().build_and_execute(|| {
			let endpoints = vec![
				(u8::from(ParsingMethod::Binance), b"https://binance.example/price".to_vec()),
				(u8::from(ParsingMethod::CoinGecko), b"https://coingecko.example/price".to_vec()),
			];
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				0u8,
				endpoints.clone(),
			));
			assert_eq!(stored_ids(0u8), endpoints);
		});
	}

	#[test]
	fn non_custom_origin_rejected() {
		ExtBuilder::default().build_and_execute(|| {
			let endpoints =
				vec![(u8::from(ParsingMethod::Binance), b"https://binance.example/price".to_vec())];
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Signed(1).into(),
					0u8,
					endpoints.clone(),
				),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::None.into(),
					0u8,
					endpoints,
				),
				DispatchError::BadOrigin,
			);
			assert!(stored_ids(0u8).is_empty());
		});
	}

	#[test]
	fn overwrites_previous() {
		ExtBuilder::default().build_and_execute(|| {
			let first = vec![(u8::from(ParsingMethod::Binance), b"a".to_vec())];
			let second = vec![
				(u8::from(ParsingMethod::Kraken), b"b".to_vec()),
				(u8::from(ParsingMethod::Okx), b"c".to_vec()),
			];
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				0u8,
				first,
			));
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				0u8,
				second.clone(),
			));
			assert_eq!(stored_ids(0u8), second);
		});
	}

	#[test]
	fn rejects_too_many_endpoints() {
		ExtBuilder::default().build_and_execute(|| {
			// MaxEndpoints = 20 in the mock runtime.
			let endpoints: Vec<_> =
				(0..21).map(|_| (u8::from(ParsingMethod::Binance), b"x".to_vec())).collect();
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					0u8,
					endpoints,
				),
				pallet::Error::<Test>::TooManyEndpoints,
			);
		});
	}

	#[test]
	fn rejects_url_too_long() {
		ExtBuilder::default().build_and_execute(|| {
			// MaxUrlLength = 64 in the mock runtime.
			let long_url = vec![b'x'; 65];
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					0u8,
					vec![(u8::from(ParsingMethod::Binance), long_url)],
				),
				pallet::Error::<Test>::UrlTooLong,
			);
		});
	}

	#[test]
	fn rejects_unknown_parsing_method() {
		ExtBuilder::default().build_and_execute(|| {
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					0u8,
					vec![(99u8, b"https://example/price".to_vec())],
				),
				pallet::Error::<Test>::UnknownParsingMethod,
			);
		});
	}
}
