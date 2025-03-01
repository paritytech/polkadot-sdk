use frame::testing_prelude::*;
use frame::deps::sp_runtime::testing::UintAuthorityId;
use pallet_staking::NullIdentity;

construct_runtime! {
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		ParasOrigin: polkadot_runtime_parachains::origin,

		Session: pallet_session,
		SessionHistorical: pallet_session::historical,
		StakingAhClient: pallet_staking_ah_client,
	}
}

pub fn roll_next() {
	let now = System::block_number();
	let next = now + 1;

	System::set_block_number(next);

	Session::on_initialize(next);
	StakingAhClient::on_initialize(next);
}

pub fn roll_until_matches(criteria: impl Fn() -> bool) {
	while !criteria() {
		roll_next();
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
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountId = frame::runtime::types_common::AccountId;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
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
	pub static Period: BlockNumber = 10;
	pub static Offset: BlockNumber = 0;
}

// TODO: tsvetimor/ankan to check this
pub struct FullIdentificationOf;
impl Convert<AccountId, Option<()>> for FullIdentificationOf {
	fn convert(_: AccountId) -> Option<()> {
		Some(Default::default())
	}
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = ();
	type FullIdentificationOf = FullIdentificationOf;
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

parameter_types! {
	pub static AssetHubId: u32 = 42;
}

parameter_types! {
	pub static XcmQueue: Vec<xcm::v5::Xcm<()>> = Default::default();
}

pub struct RcMockXCM;
impl xcm::v5::SendXcm for RcMockXCM {
	type Ticket = xcm::v5::Xcm<RuntimeCall>;

	fn deliver(
		ticket: Self::Ticket,
	) -> std::result::Result<xcm::prelude::XcmHash, xcm::prelude::SendError> {
		let mut queue = XcmQueue::get();
		queue.push(ticket.clone());
		XcmQueue::set(queue);
		Ok(ticket.using_encoded(frame::hashing::blake2_256))
	}

	fn validate(
		destination: &mut Option<xcm::prelude::Location>,
		message: &mut Option<Self::Ticket>,
	) -> xcm::prelude::SendResult<Self::Ticket> {
		let message = message.take().unwrap();

		// TODO: check destination to be RC.
		let destination = destination.take().unwrap();
		let assets = Default::default();

		Ok((message, assets))
	}
}

// needed because of the `RuntimeOrigin` of `pallet_staking_ah_client`
impl polkadot_runtime_parachains::origin::Config for Runtime {}

impl pallet_staking_ah_client::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type AssetHubId = AssetHubId;
	type CurrencyBalance = Balance;
	type SendXcm = RcMockXCM;
}

pub struct ExtBuilder;

impl Default for ExtBuilder {
	fn default() -> Self {
		Self
	}
}

impl ExtBuilder {
	pub fn build(self) -> TestState {
		let _ = sp_tracing::try_init_simple();
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		t.into()
	}
}
