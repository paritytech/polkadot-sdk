use alloc::borrow::Cow;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU32, ConstU64, Contains, EqualPrivilegeOnly, OnInitialize, OriginTrait},
	weights::Weight,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_conviction_voting::Conviction;
use pallet_referenda::Track;
use sp_runtime::{str_array as s, traits::BlakeTwo256, BuildStorage, Perbill};

extern crate alloc;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Referenda: pallet_referenda,
		ConvictionVoting: pallet_conviction_voting,
	}
);

pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		!matches!(call, &RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }))
	}
}

parameter_types! {
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = BaseFilter;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<u64>;
	type Consideration = ();
}

impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaxWeight;
	type ScheduleOrigin = EnsureRoot<u64>;
	type MaxScheduledPerBlock = ConstU32<10_000>;
	type WeightInfo = ();
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub static AlarmInterval: u64 = 1;
}

ord_parameter_types! {
	pub const Four: u64 = 4;
}

pub struct TestTracksInfo;
impl pallet_referenda::TracksInfo<u64, u64> for TestTracksInfo {
	type Id = u8;
	type RuntimeOrigin = <RuntimeOrigin as OriginTrait>::PalletsOrigin;

	fn tracks() -> impl Iterator<Item = Cow<'static, Track<Self::Id, u64, u64>>> {
		static DATA: [Track<u8, u64, u64>; 2] = [
			Track {
				id: 0u8,
				info: pallet_referenda::TrackInfo {
					name: s("root"),
					max_deciding: 1,
					decision_deposit: 10,
					prepare_period: 4,
					decision_period: 4,
					confirm_period: 2,
					min_enactment_period: 4,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			},
			Track {
				id: 1u8,
				info: pallet_referenda::TrackInfo {
					name: s("none"),
					max_deciding: 3,
					decision_deposit: 1,
					prepare_period: 2,
					decision_period: 2,
					confirm_period: 1,
					min_enactment_period: 2,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(95),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
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
				_ => Err(()),
			}
		} else {
			Err(())
		}
	}
}

impl pallet_referenda::Config for Test {
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	type SubmitOrigin = frame_system::EnsureSigned<u64>;
	type CancelOrigin = EnsureSignedBy<Four, u64>;
	type KillOrigin = EnsureRoot<u64>;
	type Slash = ();
	type Votes = pallet_conviction_voting::VotesOf<Test>;
	type Tally = pallet_conviction_voting::TallyOf<Test>;
	type SubmissionDeposit = ConstU64<2>;
	type MaxQueued = ConstU32<3>;
	type UndecidingTimeout = ConstU64<20>;
	type AlarmInterval = AlarmInterval;
	type Tracks = TestTracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

impl pallet_conviction_voting::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type VoteLockingPeriod = ConstU64<3>;
	type MaxVotes = ConstU32<64>;
	type MaxTurnout = frame_support::traits::TotalIssuanceOf<Balances, u64>;
	type Polls = Referenda;
	type BlockNumberProvider = System;
	type VotingHooks = ();
}

pub const N_ACCOUNTS: u64 = 6;
pub const INITIAL_BALANCE: u64 = 1_000_000_000;

pub fn build_genesis() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let balances: Vec<(u64, u64)> = (1..=N_ACCOUNTS).map(|i| (i, INITIAL_BALANCE)).collect();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn next_block() {
	System::set_block_number(System::block_number() + 1);
	System::reset_events();
	Scheduler::on_initialize(System::block_number());
}

pub fn set_balance_proposal_bounded(
	value: u64,
) -> frame_support::traits::Bounded<RuntimeCall, BlakeTwo256> {
	let c = RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
		who: 42,
		new_free: value,
	});
	<Preimage as frame_support::traits::StorePreimage>::bound(c).unwrap()
}

pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
    Referenda::do_try_state()
}
