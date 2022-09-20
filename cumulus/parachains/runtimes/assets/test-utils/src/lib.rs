use frame_support::traits::GenesisBuild;
use sp_std::marker::PhantomData;

use frame_support::traits::OriginTrait;
use parachains_common::AccountId;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_core::Encode;
use sp_runtime::{Digest, DigestItem};

pub type BalanceOf<Runtime> = <Runtime as pallet_balances::Config>::Balance;
pub type AccountIdOf<Runtime> = <Runtime as frame_system::Config>::AccountId;
pub type ValidatorIdOf<Runtime> = <Runtime as pallet_session::Config>::ValidatorId;
pub type SessionKeysOf<Runtime> = <Runtime as pallet_session::Config>::Keys;

// Basic builder based on balances, collators and pallet_sessopm
pub struct ExtBuilder<
	Runtime: frame_system::Config + pallet_balances::Config + pallet_session::Config,
> {
	// endowed accounts with balances
	balances: Vec<(AccountIdOf<Runtime>, BalanceOf<Runtime>)>,
	// collators to test block prod
	collators: Vec<AccountIdOf<Runtime>>,
	// keys added to pallet session
	keys: Vec<(AccountIdOf<Runtime>, ValidatorIdOf<Runtime>, SessionKeysOf<Runtime>)>,
	_runtime: PhantomData<Runtime>,
}

impl<Runtime: frame_system::Config + pallet_balances::Config + pallet_session::Config> Default
	for ExtBuilder<Runtime>
{
	fn default() -> ExtBuilder<Runtime> {
		ExtBuilder { balances: vec![], collators: vec![], keys: vec![], _runtime: PhantomData }
	}
}

impl<Runtime: frame_system::Config + pallet_balances::Config + pallet_session::Config>
	ExtBuilder<Runtime>
{
	pub fn with_balances(
		mut self,
		balances: Vec<(AccountIdOf<Runtime>, BalanceOf<Runtime>)>,
	) -> Self {
		self.balances = balances;
		self
	}
	pub fn with_collators(mut self, collators: Vec<AccountIdOf<Runtime>>) -> Self {
		self.collators = collators;
		self
	}

	pub fn with_session_keys(
		mut self,
		keys: Vec<(AccountIdOf<Runtime>, ValidatorIdOf<Runtime>, SessionKeysOf<Runtime>)>,
	) -> Self {
		self.keys = keys;
		self
	}

	pub fn build(self) -> sp_io::TestExternalities
	where
		Runtime:
			pallet_collator_selection::Config + pallet_balances::Config + pallet_session::Config,
		ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	{
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

		pallet_balances::GenesisConfig::<Runtime> { balances: self.balances.into() }
			.assimilate_storage(&mut t)
			.unwrap();

		pallet_collator_selection::GenesisConfig::<Runtime> {
			invulnerables: self.collators.clone().into(),
			candidacy_bond: Default::default(),
			desired_candidates: Default::default(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_session::GenesisConfig::<Runtime> { keys: self.keys }
			.assimilate_storage(&mut t)
			.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);

		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
		});

		ext
	}
}

pub struct RuntimeHelper<Runtime>(PhantomData<Runtime>);
/// Utility function that advances the chain to the desired block number.
/// If an author is provided, that author information is injected to all the blocks in the meantime.
impl<Runtime: frame_system::Config> RuntimeHelper<Runtime>
where
	AccountIdOf<Runtime>:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
{
	pub fn run_to_block(n: u32, author: Option<AccountId>) {
		while frame_system::Pallet::<Runtime>::block_number() < n.into() {
			// Set the new block number and author
			match author {
				Some(ref author) => {
					let pre_digest = Digest {
						logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, author.encode())],
					};
					frame_system::Pallet::<Runtime>::reset_events();
					frame_system::Pallet::<Runtime>::initialize(
						&(frame_system::Pallet::<Runtime>::block_number() + 1u32.into()),
						&frame_system::Pallet::<Runtime>::parent_hash(),
						&pre_digest,
					);
				},
				None => {
					frame_system::Pallet::<Runtime>::set_block_number(
						frame_system::Pallet::<Runtime>::block_number() + 1u32.into(),
					);
				},
			}
		}
	}

	pub fn root_origin() -> <Runtime as frame_system::Config>::RuntimeOrigin {
		<Runtime as frame_system::Config>::RuntimeOrigin::root()
	}

	pub fn origin_of(
		account_id: AccountIdOf<Runtime>,
	) -> <Runtime as frame_system::Config>::RuntimeOrigin {
		<Runtime as frame_system::Config>::RuntimeOrigin::signed(account_id.into())
	}
}
