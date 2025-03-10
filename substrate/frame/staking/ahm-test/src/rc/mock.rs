use frame::{deps::sp_runtime::testing::UintAuthorityId, testing_prelude::*};
use frame::traits::fungible::Mutate;
use pallet_staking_ah_client as ah_client;
use sp_staking::SessionIndex;

use crate::shared;

construct_runtime! {
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,

		Session: pallet_session,
		SessionHistorical: pallet_session::historical,
		StakingAhClient: pallet_staking_ah_client,
	}
}

pub fn roll_next() {
	let now = System::block_number();
	let next = now + 1;

	System::set_block_number(next);
	// Timestamp is always the RC block number * 1000
	Timestamp::set_timestamp(next * 1000);

	Session::on_initialize(next);
	StakingAhClient::on_initialize(next);
}

pub fn roll_until_matches(criteria: impl Fn() -> bool, with_ah: bool) {
	while !criteria() {
		roll_next();
		if with_ah {
			if LocalQueue::get().is_some() {
				panic!("when local queue is set, you cannot roll ah forward as well!")
			}
			shared::in_ah(|| {
				crate::ah::roll_next();
			});
		}
	}
}

pub type AccountId = <Runtime as frame_system::Config>::AccountId;
pub type Balance = <Runtime as pallet_balances::Config>::Balance;
pub type Hash = <Runtime as frame_system::Config>::Hash;
pub type BlockNumber = BlockNumberFor<Runtime>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = MockBlock<Self>;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<3>;
	type WeightInfo = ();
}

pub struct ValidatorIdOf;
impl Convert<AccountId, Option<AccountId>> for ValidatorIdOf {
	fn convert(a: AccountId) -> Option<AccountId> {
		Some(a)
	}
}

pub struct OtherSessionHandler;
impl OneSessionHandler<AccountId> for OtherSessionHandler {
	type Key = UintAuthorityId;

	fn on_genesis_session<'a, I: 'a>(_: I)
	where
		I: Iterator<Item = (&'a AccountId, Self::Key)>,
		AccountId: 'a,
	{
	}

	fn on_new_session<'a, I: 'a>(_: bool, _: I, _: I)
	where
		I: Iterator<Item = (&'a AccountId, Self::Key)>,
		AccountId: 'a,
	{
	}

	fn on_disabled(_validator_index: u32) {}
}

impl BoundToRuntimeAppPublic for OtherSessionHandler {
	type Public = UintAuthorityId;
}

frame::deps::sp_runtime::impl_opaque_keys! {
	pub struct SessionKeys {
		pub other: OtherSessionHandler,
	}
}

parameter_types! {
	pub static Period: BlockNumber = 30;
	pub static Offset: BlockNumber = 0;
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = ();
	type FullIdentificationOf = pallet_staking::NullIdentity;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type ValidatorIdOf = ValidatorIdOf;
	type ValidatorId = AccountId;

	type DisablingStrategy = ();

	type Keys = SessionKeys;
	type SessionHandler = <SessionKeys as frame::traits::OpaqueKeys>::KeyTypeIdProviders;

	type NextSessionRotation = Self::ShouldEndSession;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;

	// Should be AH-client
	type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, StakingAhClient>;

	type WeightInfo = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingMessages {
	SessionReport(rc_client::SessionReport<AccountId>),
	OffenceReport(SessionIndex, Vec<rc_client::Offence<AccountId>>),
}

parameter_types! {
	pub static MinimumValidatorSetSize: u32 = 4;
	pub static LocalQueue: Option<Vec<(BlockNumber, OutgoingMessages)>> = None;
}

impl ah_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendToAssetHub = DeliverToAH;
	type AssetHubOrigin = EnsureSigned<AccountId>;
	type UnixTime = Timestamp;
	type MinimumValidatorSetSize = MinimumValidatorSetSize;
	type PointsPerBlock = ConstU32<20>;
}

use pallet_staking_rc_client as rc_client;
pub struct DeliverToAH;
impl ah_client::SendToAssetHub for DeliverToAH {
	type AccountId = AccountId;
	fn relay_new_offence(
		session_index: SessionIndex,
		offences: Vec<rc_client::Offence<Self::AccountId>>,
	) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue.push((
				System::block_number(),
				OutgoingMessages::OffenceReport(session_index, offences),
			));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::in_ah(|| {
				let origin = crate::ah::RuntimeOrigin::root();
				rc_client::Pallet::<crate::ah::Runtime>::relay_new_offence(
					origin,
					session_index,
					offences.clone(),
				)
				.unwrap();
			});
		}
	}

	fn relay_session_report(session_report: rc_client::SessionReport<Self::AccountId>) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue
				.push((System::block_number(), OutgoingMessages::SessionReport(session_report)));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::in_ah(|| {
				let origin = crate::ah::RuntimeOrigin::root();
				rc_client::Pallet::<crate::ah::Runtime>::relay_session_report(
					origin,
					session_report.clone(),
				)
				.unwrap();
			});
		}
	}
}

pub struct ExtBuilder {
	session_keys: Vec<AccountId>
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { session_keys: vec![] }
	}
}

impl ExtBuilder {
	/// Set this if you want to test the rc-runtime locally. This will push outgoing messages to
	/// `LocalQueue` instead of enacting them on AH.
	pub fn local_queue(self) -> Self {
		LocalQueue::set(Some(Default::default()));
		self
	}

	/// Set the session keys for the given accounts.
	pub fn session_keys(mut self, session_keys: Vec<AccountId>) -> Self {
		self.session_keys = session_keys;
		self
	}

	pub fn build(self) -> TestState {
		let _ = sp_tracing::try_init_simple();
		let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		let mut state: TestState = t.into();
		state.execute_with(|| {
			// so events can be deposited.
			frame_system::Pallet::<Runtime>::set_block_number(1);

			for v in self.session_keys {
				// min some funds, create account and ref counts
				pallet_balances::Pallet::<Runtime>::mint_into(&v, 1).unwrap();
				pallet_session::Pallet::<Runtime>::set_keys(
					RuntimeOrigin::signed(v),
					SessionKeys { other: UintAuthorityId(v) },
					vec![],
				)
				.unwrap();
			}
		});

		state
	}
}
