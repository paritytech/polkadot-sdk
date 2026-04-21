use super::*;
use crate as pallet_price_oracle;

use alloc::vec;
use frame_support::{assert_noop, assert_ok, derive_impl, parameter_types};
use sp_consensus_babe::{AuthorityId, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_core::{crypto::Pair as PairT, sr25519};
use sp_inherents::InherentData;
use sp_io::TestExternalities;
use sp_price_oracle::{Nudge, SignedNudge, INHERENT_IDENTIFIER};
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
	pub const Epsilon: FixedU128 = FixedU128::from_rational(1, 100); // 0.01
	pub const NudgeValidity: u64 = 10;
	pub static MinNudges: u32 = 0;
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
	type PriceOracleOrigin = frame_system::EnsureRoot<u64>;
}

struct ExtBuilder {
	num_authorities: usize,
	current_slot: u64,
	min_nudges: u32,
	initial_price: Option<FixedU128>,
	panic_switch: bool,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		set_authorities(&generate_test_pairs(3));
		Self {
			num_authorities: 3,
			current_slot: 5,
			min_nudges: 0,
			initial_price: None,
			panic_switch: false,
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

	fn min_nudges(mut self, min: u32) -> Self {
		self.min_nudges = min;
		self
	}

	fn initial_price(mut self, price: FixedU128) -> Self {
		self.initial_price = Some(price);
		self
	}

	fn panic_switch(mut self, panic_switch: bool) -> Self {
		self.panic_switch = panic_switch;
		self
	}

	fn build(self) -> TestExternalities {
		let pairs = generate_test_pairs(self.num_authorities);
		set_authorities(&pairs);
		set_current_slot(self.current_slot);
		MinNudges::set(self.min_nudges);

		let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext = TestExternalities::new(t);

		ext.execute_with(|| {
			if let Some(price) = self.initial_price {
				pallet::CurrentPrice::<Test>::put(price);
			}
			if self.panic_switch {
				pallet::PanicSwitch::<Test>::put(true);
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
		assert_eq!(PriceOracle::current_price(), FixedU128::zero());
	});
}

#[test]
fn single_up_nudge_increases_price_by_epsilon() {
	ExtBuilder::default().build_and_execute(|| {
		let pairs = generate_test_pairs(3);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge],));

		let expected = FixedU128::from_rational(1, 100);
		assert_eq!(PriceOracle::current_price(), expected);
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
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 3 ups, 0 downs → net 3 → price = 3 * 0.01 = 0.03
		let expected = FixedU128::from_rational(3, 100);
		assert_eq!(PriceOracle::current_price(), expected);
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
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		// 3 ups, 1 down → net 2 up → price = 2 * 0.01 = 0.02
		let expected = FixedU128::from_rational(2, 100);
		assert_eq!(PriceOracle::current_price(), expected);
	});
}

#[test]
fn down_nudges_decrease_price() {
	ExtBuilder::default()
		.initial_price(FixedU128::from_u32(1))
		.build_and_execute(|| {
			let pairs = generate_test_pairs(3);
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
fn stale_nudge_returns_error() {
	ExtBuilder::default().authorities(2).current_slot(15).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 15, 0), // valid
			make_signed_nudge(&pairs[1], Nudge::Up, 4, 1),  // stale (15 - 4 = 11 >= 10)
		];
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges),
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
				vec![bad_nudge, good_nudge],
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
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![bad_nudge]),
			Error::<Test>::InvalidSignature
		);
	});
}

#[test]
fn price_cannot_go_below_zero() {
	ExtBuilder::default().authorities(1).current_slot(5).build_and_execute(|| {
		let pairs = generate_test_pairs(1);
		// Price starts at 0, pushing down should stay at 0
		let nudge = make_signed_nudge(&pairs[0], Nudge::Down, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge],));
	});
}

#[test]
fn empty_nudges_does_not_change_price() {
	ExtBuilder::default()
		.initial_price(FixedU128::from_u32(5))
		.build_and_execute(|| {
			assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![],));
		});
}

#[test]
fn submit_nudges_only_once_per_block() {
	ExtBuilder::default().authorities(2).build_and_execute(|| {
		let pairs = generate_test_pairs(2);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge],));

		// Second submission in the same block should return error
		let nudge2 = make_signed_nudge(&pairs[1], Nudge::Up, 5, 1);
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge2]),
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
			make_signed_nudge(&pairs[0], Nudge::Up, 4, 0), // same authority_index=0
			make_signed_nudge(&pairs[1], Nudge::Up, 5, 1),
		];
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges),
			Error::<Test>::DuplicateNudge
		);
	});
}

#[test]
fn nudge_count_tracks_valid_nudges() {
	ExtBuilder::default().current_slot(15).build_and_execute(|| {
		let pairs = generate_test_pairs(3);
		let nudges = vec![
			make_signed_nudge(&pairs[0], Nudge::Up, 15, 0),
			make_signed_nudge(&pairs[1], Nudge::Up, 14, 1),
			make_signed_nudge(&pairs[2], Nudge::Up, 6, 2), // valid: 6+10=16 > 15
		];
		assert_ok!(PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), nudges,));

		assert_eq!(pallet::NudgeCount::<Test>::get(), Some(3));
	});
}

#[test]
fn too_few_nudges_returns_error() {
	ExtBuilder::default().min_nudges(2).build_and_execute(|| {
		let pairs = generate_test_pairs(3);
		let nudge = make_signed_nudge(&pairs[0], Nudge::Up, 5, 0);
		assert_noop!(
			PriceOracle::submit_nudges(frame_system::RawOrigin::None.into(), vec![nudge]),
			Error::<Test>::TooFewNudges,
		);
	});
}

#[test]
#[should_panic]
fn panic_switch_on_without_inherent_panics() {
	ExtBuilder::default().panic_switch(true).current_slot(1).build_and_execute(|| {
		NudgeCount::<Test>::set(None);
		PriceOracle::on_finalize(1);
	});
}

#[test]
fn bad_origin_set_panic_switch_returns_error() {
	ExtBuilder::default().panic_switch(true).build_and_execute(|| {
		assert_noop!(
			PriceOracle::set_panic_switch(frame_system::RawOrigin::Signed(1).into(), true),
			BadOrigin
		);
	});
}

/// Tests for the full inherent pipeline: node-side data → runtime processing.
///
/// In production, the pipeline is:
///
/// 1. **Node gossip service** collects signed nudges from peers into `NudgeStore`
/// 2. **Node inherent provider** (`create_inherent_data`) selects a subset from the store and packs
///    them into `sp_inherents::InherentData` via `sp_price_oracle::INHERENT_IDENTIFIER`
/// 3. **Runtime `create_inherent`** deserializes the `InherentData` into `Call::submit_nudges`
/// 4. **Runtime `check_inherent`** validates signatures and freshness (import-time rejection)
/// 5. **Runtime `submit_nudges`** executes: verifies sigs, filters stale/duplicate/invalid, counts
///    ups vs downs, applies epsilon to update `CurrentPrice`
///
/// These tests cover steps 3–5 by constructing `InherentData` directly and running it through
/// `create_inherent` → dispatch. This catches mismatches between what the node side produces
/// and what the runtime accepts (e.g. the duplicate-authority bug where the node could pass
/// two nudges from the same validator, which would have caused the runtime to count them twice).
mod inherent_pipeline {
	use super::*;
	use frame_support::{pallet_prelude::ProvideInherent, traits::UnfilteredDispatchable};

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

	fn run_inherent_expect_err(nudges: Vec<SignedNudge>, expected: Error<Test>) {
		let data = build_inherent_data(nudges);
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
			run_inherent(nudges);

			// 2 ups, 1 down → net 1 up → price = 0.01
			assert_eq!(PriceOracle::current_price(), FixedU128::from_rational(1, 100));
			assert_eq!(pallet::NudgeCount::<Test>::get(), Some(3));
		});
	}

	#[test]
	fn duplicates_return_error() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 10, 0),
				make_signed_nudge(&pairs[0], Nudge::Up, 9, 0), // duplicate authority
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent_expect_err(nudges, Error::<Test>::DuplicateNudge);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
		});
	}

	#[test]
	fn stale_nudge_returns_error() {
		ExtBuilder::default().authorities(2).current_slot(20).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 20, 0),
				make_signed_nudge(&pairs[1], Nudge::Up, 5, 1), // 5+10=15 <= 20 → stale
			];
			run_inherent_expect_err(nudges, Error::<Test>::StaleNudge);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
		});
	}

	#[test]
	fn bad_signature_returns_error() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 0), // wrong key for auth 0
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 1),
			];
			run_inherent_expect_err(nudges, Error::<Test>::InvalidSignature);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
		});
	}

	#[test]
	fn empty_inherent() {
		ExtBuilder::default()
			.initial_price(FixedU128::from_u32(5))
			.build_and_execute(|| {
				run_inherent(vec![]);

				assert_eq!(PriceOracle::current_price(), FixedU128::from_u32(5));
				assert_eq!(pallet::NudgeCount::<Test>::get(), Some(0));
			});
	}

	#[test]
	fn stale_nudge_is_rejected() {
		ExtBuilder::default().authorities(1).current_slot(100).build_and_execute(|| {
			let pairs = generate_test_pairs(1);
			let nudges = vec![
				make_signed_nudge(&pairs[0], Nudge::Up, 1, 0), // stale
			];
			run_inherent_expect_err(nudges, Error::<Test>::StaleNudge);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
		});
	}

	#[test]
	fn invalid_authority_is_rejected() {
		ExtBuilder::default().authorities(2).current_slot(10).build_and_execute(|| {
			let pairs = generate_test_pairs(2);
			let nudges = vec![
				make_signed_nudge(&pairs[1], Nudge::Up, 10, 3), // authority index out of range
			];
			run_inherent_expect_err(nudges, Error::<Test>::InvalidAuthority);

			assert_eq!(PriceOracle::current_price(), FixedU128::zero());
		});
	}
}
