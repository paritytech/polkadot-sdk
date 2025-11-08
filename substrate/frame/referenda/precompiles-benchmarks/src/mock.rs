#![cfg(all(feature = "runtime-benchmarks", feature = "std"))]

use pallet_referenda_precompiles::ReferendaPrecompile;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::TypeInfo;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, Contains, EqualPrivilegeOnly, OriginTrait, VoteTally},
	weights::Weight,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_referenda::{Curve, Track, TrackInfo, TracksInfo};
use sp_runtime::{str_array as s, traits::IdentityLookup, AccountId32, BuildStorage, Perbill};
use std::borrow::Cow;

type Block = frame_system::mocking::MockBlock<Test>;

pub type AccountId = AccountId32;
pub type Balance = u128;

// Re-export account constants that might be needed
pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Referenda: pallet_referenda,
		Revive: pallet_revive,
	}
);

parameter_types! {
	pub const MinimumPeriod: u64 = 1;
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
	pub ExistentialDeposit: Balance = 1;
	pub static AlarmInterval: u64 = 1;
}

ord_parameter_types! {
	pub const FourAccount: AccountId = AccountId32::new([4u8; 32]);
}

pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(_call: &RuntimeCall) -> bool {
		true
	}
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = BaseFilter;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
	type Balance = Balance;
	type Currency = Balances;
	type Precompiles = (ReferendaPrecompile<Self>,);
	type Time = Timestamp;
	type UploadOrigin = frame_system::EnsureSigned<AccountId>;
	type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = ();
}

impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaxWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	type MaxScheduledPerBlock = ConstU32<100>;
	type WeightInfo = ();
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

#[derive(
	Encode, Debug, Decode, DecodeWithMemTracking, TypeInfo, Eq, PartialEq, Clone, MaxEncodedLen,
)]
pub struct Tally {
	pub ayes: u32,
	pub nays: u32,
}

pub struct TestTracksInfo;

impl pallet_referenda::Config for Test {
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = pallet_balances::Pallet<Self>;
	type SubmitOrigin = frame_system::EnsureSigned<AccountId>;
	type CancelOrigin = EnsureSignedBy<FourAccount, AccountId>;
	type KillOrigin = EnsureRoot<AccountId>;
	type Slash = ();
	type Votes = u32;
	type Tally = Tally;
	type SubmissionDeposit = ConstU128<2>;
	type MaxQueued = ConstU32<3>;
	type UndecidingTimeout = ConstU64<20>;
	type AlarmInterval = AlarmInterval;
	type Tracks = TestTracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

impl TracksInfo<Balance, u64> for TestTracksInfo {
	type Id = u8;
	type RuntimeOrigin = <RuntimeOrigin as OriginTrait>::PalletsOrigin;

	fn tracks() -> impl Iterator<Item = Cow<'static, Track<Self::Id, Balance, u64>>> {
		static DATA: [Track<u8, Balance, u64>; 3] = [
			Track {
				id: 0u8,
				info: TrackInfo {
					name: s("root"),
					max_deciding: 1,
					decision_deposit: 10,
					prepare_period: 4,
					decision_period: 4,
					confirm_period: 2,
					min_enactment_period: 4,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			},
			Track {
				id: 1u8,
				info: TrackInfo {
					name: s("none"),
					max_deciding: 3,
					decision_deposit: 1,
					prepare_period: 2,
					decision_period: 2,
					confirm_period: 1,
					min_enactment_period: 2,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(95),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(90),
						ceil: Perbill::from_percent(100),
					},
				},
			},
			Track {
				id: 2u8,
				info: TrackInfo {
					name: s("none"),
					max_deciding: 3,
					decision_deposit: 1,
					prepare_period: 2,
					decision_period: 2,
					confirm_period: 1,
					min_enactment_period: 0,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(95),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(90),
						ceil: Perbill::from_percent(100),
					},
				},
			},
		];
		DATA.iter().map(Cow::Borrowed)
	}

	fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		if let Ok(system_origin) = frame_system::RawOrigin::try_from(id.clone()) {
			match system_origin {
				frame_system::RawOrigin::Root => Ok(0),
				frame_system::RawOrigin::None => Ok(1),
				frame_system::RawOrigin::Signed(_) => Ok(2),
				_ => Err(()),
			}
		} else {
			Err(())
		}
	}
}

impl VoteTally<u32, u8> for Tally {
	fn new(_: u8) -> Self {
		Self { ayes: 0, nays: 0 }
	}

	fn ayes(&self, _: u8) -> u32 {
		self.ayes
	}

	fn support(&self, _: u8) -> Perbill {
		Perbill::from_percent(self.ayes)
	}

	fn approval(&self, _: u8) -> Perbill {
		if self.ayes + self.nays > 0 {
			Perbill::from_rational(self.ayes, self.ayes + self.nays)
		} else {
			Perbill::zero()
		}
	}

	fn unanimity(_: u8) -> Self {
		Self { ayes: 100, nays: 0 }
	}

	fn rejection(_: u8) -> Self {
		Self { ayes: 0, nays: 100 }
	}

	fn from_requirements(support: Perbill, approval: Perbill, _: u8) -> Self {
		let ayes = support.mul_ceil(100u32);
		let nays = ((ayes as u64) * 1_000_000_000u64 / approval.deconstruct() as u64) as u32 - ayes;
		Self { ayes, nays }
	}

	fn setup(_: u8, _: Perbill) {}
}

impl crate::pallet::Config for Test {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let balances = vec![(ALICE, 100), (BOB, 100)];
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_revive::GenesisConfig::<Test> {
		mapped_accounts: vec![ALICE, BOB],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| frame_system::Pallet::<Test>::set_block_number(1));
	ext
}
