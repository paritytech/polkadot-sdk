use super::*;
use crate as pallet_price_oracle;

use alloc::vec;
use frame_support::{assert_noop, assert_ok, derive_impl, parameter_types};
use sp_consensus_babe::{AuthorityId, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_core::{crypto::Pair as PairT, sr25519};
use sp_inherents::InherentData;
use sp_io::TestExternalities;
use pallet_price_oracle::ParsingMethod;
use sp_price_oracle::{Nudge, SignedNudge, INHERENT_IDENTIFIER};
use sp_runtime::{BuildStorage, FixedU128};

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
	pub const Epsilon: FixedU128 = FixedU128::from_rational(1, 100); // 0.01
	pub const MinNudges: u32 = 0;
	pub const NudgeValidity: u64 = 10;
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
	type Epsilon = Epsilon;
	type MinNudges = MinNudges;
	type NudgeValidity = NudgeValidity;
	type AuthorityProvider = MockAuthorityProvider;
	type TimeProvider = MockTime;
	type OnPriceUpdate = ();
	type MaxEndpoints = MaxEndpoints;
	type MaxUrlLength = MaxUrlLength;
}

fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	TestExternalities::new(t)
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
	new_test_ext().execute_with(|| {
		assert_eq!(PriceOracle::current_price(), FixedU128::zero());
	});
}

#[test]
fn single_up_nudge_increases_price_by_epsilon() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(3);
		set_authorities(&pairs);
		set_current_slot(5);

		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge],));

		let expected = FixedU128::from_rational(1, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn multiple_ups_compound() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(3);
		set_authorities(&pairs);
		set_current_slot(5);

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			make_signed_nudge(&pairs[2], Nudge::Up, 4, 2),
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 3 ups, 0 downs → net 3 → price = 3 * 0.01 = 0.03
		let expected = FixedU128::from_rational(3, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn ups_and_downs_cancel_out() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(4);
		set_authorities(&pairs);
		set_current_slot(5);

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
			make_signed_nudge(&pairs[2], Nudge::Down, 4, 2),
			make_signed_nudge(&pairs[3], Nudge::Up, 4, 3),
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 3 ups, 1 down → net 2 up → price = 2 * 0.01 = 0.02
		let expected = FixedU128::from_rational(2, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn down_nudges_decrease_price() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(3);
		set_authorities(&pairs);
		set_current_slot(5);

		// First set price to 1.0
		pallet::CurrentPrice::<Test>::put(FixedU128::from_u32(1));

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Down, 5, 0),
			make_signed_nudge(&pairs[1], Nudge::Down, 5, 1),
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 0 ups, 2 downs → net 2 down → price = 1.0 - 0.02 = 0.98
		let expected = FixedU128::from_rational(98, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn stale_nudges_are_skipped() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(2);
		set_authorities(&pairs);
		set_current_slot(15);

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 15, 0), // valid
			make_signed_nudge(&pairs[1], Nudge::Up, 4, 1),  // stale (15 - 4 = 11 >= 10)
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// Only 1 valid up nudge → price = 0.01
		let expected = FixedU128::from_rational(1, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn invalid_signature_is_skipped() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(2);
		set_authorities(&pairs);
		set_current_slot(5);

		// Sign with pair[1]'s key but claim to be authority 0
		let bad_nudge = make_signed_nudge(&pairs[1], Nudge::Up, 5, 0);
		let good_nudge = make_signed_nudge(&pairs[1], Nudge::Up, 5, 1);

		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![bad_nudge, good_nudge],
		));

		// Only 1 valid nudge
		let expected = FixedU128::from_rational(1, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn price_cannot_go_below_zero() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(1);
		set_authorities(&pairs);
		set_current_slot(5);

		// Price starts at 0, pushing down should stay at 0
		let nudge = make_signed_nudge(&pairs[0], Nudge::Down, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge],));

		assert_eq!(PriceOracle::current_price(), FixedU128::zero());
	});
}

#[test]
fn empty_nudges_does_not_change_price() {
	new_test_ext().execute_with(|| {
		pallet::CurrentPrice::<Test>::put(FixedU128::from_u32(5));

		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![],));

		assert_eq!(PriceOracle::current_price(), FixedU128::from_u32(5));
	});
}

#[test]
fn submit_nudges_only_once_per_block() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(1);
		set_authorities(&pairs);
		set_current_slot(5);

		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(
			frame_system::RawOrigin::None.into(),
			vec![nudge.clone()],
		));

		// Second submission in the same block should panic
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let _ = PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge]);
		}));
		assert!(result.is_err());
	});
}

#[test]
fn duplicate_authority_nudges_are_skipped() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(2);
		set_authorities(&pairs);
		set_current_slot(5);

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 5, 0),
			make_signed_nudge(&pairs[0], Nudge::Up, 4, 0), // same authority_index=0
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 2 valid (one from auth 0, one from auth 1), duplicate skipped
		let expected = FixedU128::from_rational(2, 100);
		assert_eq!(PriceOracle::current_price(), expected);
		assert_eq!(pallet::NudgeCount::<Test>::get(), 2);
	});
}

#[test]
fn nudge_count_tracks_valid_nudges() {
	new_test_ext().execute_with(|| {
		let pairs = generate_test_pairs(3);
		set_authorities(&pairs);
		set_current_slot(15);

		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 15, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 14, 1),
			make_signed_nudge(&pairs[2], Nudge::Up, 1, 2), // stale: 1+10=11 <= 15
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		assert_eq!(pallet::NudgeCount::<Test>::get(), 2);
	});
}

/// Tests for the full inherent pipeline: node-side data → runtime processing.
///
/// In production, the pipeline is:
///
/// 1. **Node gossip service** collects signed nudges from peers into `NudgeStore`
/// 2. **Node inherent provider** (`create_inherent_data`) selects a subset from the store
///    and packs them into `sp_inherents::InherentData` via `sp_price_oracle::INHERENT_IDENTIFIER`
/// 3. **Runtime `create_inherent`** deserializes the `InherentData` into `Call::submit_nudges`
/// 4. **Runtime `check_inherent`** validates signatures and freshness (import-time rejection)
/// 5. **Runtime `submit_nudges`** executes: verifies sigs, filters stale/duplicate/invalid,
///    counts ups vs downs, applies epsilon to update `CurrentPrice`
///
/// These tests cover steps 3–5 by constructing `InherentData` directly and running it through
/// `create_inherent` → dispatch. This catches mismatches between what the node side produces
/// and what the runtime accepts (e.g. the duplicate-authority bug where the node could pass
/// two nudges from the same validator, which would have caused the runtime to count them twice).
mod inherent_pipeline {
	use super::*;
	use frame_support::pallet_prelude::ProvideInherent;
	use frame_support::traits::UnfilteredDispatchable;

	fn build_inherent_data(nudges: Vec<SignedNudge>) -> InherentData {
		let mut data = InherentData::new();
		data.put_data(INHERENT_IDENTIFIER, &nudges).expect("puts inherent data");
		data
	}

	fn run_inherent(nudges: Vec<SignedNudge>) {
		let data = build_inherent_data(nudges);
		let call = PriceOracle::create_inherent(&data).expect("create_inherent returns Some");
		assert_ok!(call.dispatch_bypass_filter(frame_system::RawOrigin::None.into()));
	}

	#[test]
	fn happy_path() {
		let pairs = generate_test_pairs(3);
		new_test_ext().execute_with(|| {
			set_authorities(&pairs);
			set_current_slot(10);

			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 9, 1),
				make_signed_nudge(&pairs[2], Nudge::Down, 10, 2),
			];
			run_inherent(nudges);

			// 2 ups, 1 down → net 1 up → price = 0.01
			assert_eq!(PriceOracle::current_price(), FixedU128::from_rational(1, 100));
			assert_eq!(pallet::NudgeCount::<Test>::get(), 3);
		});
	}

	#[test]
	fn duplicates_do_not_panic() {
		let pairs = generate_test_pairs(2);
		new_test_ext().execute_with(|| {
			set_authorities(&pairs);
			set_current_slot(10);

			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[0], Nudge::Up, 9, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent(nudges);

			// duplicate skipped → 2 valid ups → price = 0.02
			assert_eq!(PriceOracle::current_price(), FixedU128::from_rational(2, 100));
			assert_eq!(pallet::NudgeCount::<Test>::get(), 2);
		});
	}

	#[test]
	fn stale_nudges_filtered() {
		let pairs = generate_test_pairs(2);
		new_test_ext().execute_with(|| {
			set_authorities(&pairs);
			set_current_slot(20);

			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 20, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 5, 1), // 5+10=15 <= 20 → stale
			];
			run_inherent(nudges);

			assert_eq!(PriceOracle::current_price(), FixedU128::from_rational(1, 100));
			assert_eq!(pallet::NudgeCount::<Test>::get(), 1);
		});
	}

	#[test]
	fn bad_signature_filtered() {
		let pairs = generate_test_pairs(2);
		new_test_ext().execute_with(|| {
			set_authorities(&pairs);
			set_current_slot(10);

			let nudges = vec![
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 0), // wrong key for auth 0
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent(nudges);

			assert_eq!(PriceOracle::current_price(), FixedU128::from_rational(1, 100));
			assert_eq!(pallet::NudgeCount::<Test>::get(), 1);
		});
	}

	#[test]
	fn empty_inherent() {
		new_test_ext().execute_with(|| {
			pallet::CurrentPrice::<Test>::put(FixedU128::from_u32(5));
			run_inherent(vec![]);

			assert_eq!(PriceOracle::current_price(), FixedU128::from_u32(5));
			assert_eq!(pallet::NudgeCount::<Test>::get(), 0);
		});
	}

	#[test]
	fn all_invalid() {
		let pairs = generate_test_pairs(2);
		new_test_ext().execute_with(|| {
			set_authorities(&pairs);
			set_current_slot(100);

			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 1, 0),  // stale
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 3), // invalid authority index
			];
			run_inherent(nudges);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
			assert_eq!(pallet::NudgeCount::<Test>::get(), 0);
		});
	}
}

mod active_endpoints {
	use super::*;
	use sp_runtime::DispatchError;

	fn stored_ids() -> Vec<(u8, Vec<u8>)> {
		pallet::ActiveEndpoints::<Test>::get()
			.into_iter()
			.map(|(m, url)| (m.into(), url.into_inner()))
			.collect()
	}

	#[test]
	fn starts_empty() {
		new_test_ext().execute_with(|| {
			assert!(pallet::ActiveEndpoints::<Test>::get().is_empty());
		});
	}

	#[test]
	fn root_can_set() {
		new_test_ext().execute_with(|| {
			let endpoints = vec![
				(u8::from(ParsingMethod::Binance), b"https://binance.example/price".to_vec()),
				(u8::from(ParsingMethod::CoinGecko), b"https://coingecko.example/price".to_vec()),
			];
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				endpoints.clone(),
			));
			assert_eq!(stored_ids(), endpoints);
		});
	}

	#[test]
	fn non_root_rejected() {
		new_test_ext().execute_with(|| {
			let endpoints =
				vec![(u8::from(ParsingMethod::Binance), b"https://binance.example/price".to_vec())];
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Signed(1).into(),
					endpoints.clone(),
				),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::None.into(),
					endpoints,
				),
				DispatchError::BadOrigin,
			);
			assert!(pallet::ActiveEndpoints::<Test>::get().is_empty());
		});
	}

	#[test]
	fn overwrites_previous() {
		new_test_ext().execute_with(|| {
			let first = vec![(u8::from(ParsingMethod::Binance), b"a".to_vec())];
			let second = vec![
				(u8::from(ParsingMethod::Kraken), b"b".to_vec()),
				(u8::from(ParsingMethod::Okx), b"c".to_vec()),
			];
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				first,
			));
			assert_ok!(PriceOracle::set_active_endpoints(
				frame_system::RawOrigin::Root.into(),
				second.clone(),
			));
			assert_eq!(stored_ids(), second);
		});
	}

	#[test]
	fn rejects_too_many_endpoints() {
		new_test_ext().execute_with(|| {
			// MaxEndpoints = 20 in the mock runtime.
			let endpoints: Vec<_> =
				(0..21).map(|_| (u8::from(ParsingMethod::Binance), b"x".to_vec())).collect();
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					endpoints,
				),
				pallet::Error::<Test>::TooManyEndpoints,
			);
		});
	}

	#[test]
	fn rejects_url_too_long() {
		new_test_ext().execute_with(|| {
			// MaxUrlLength = 64 in the mock runtime.
			let long_url = vec![b'x'; 65];
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					vec![(u8::from(ParsingMethod::Binance), long_url)],
				),
				pallet::Error::<Test>::UrlTooLong,
			);
		});
	}

	#[test]
	fn rejects_unknown_parsing_method() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				PriceOracle::set_active_endpoints(
					frame_system::RawOrigin::Root.into(),
					vec![(99u8, b"https://example/price".to_vec())],
				),
				pallet::Error::<Test>::UnknownParsingMethod,
			);
		});
	}
}
