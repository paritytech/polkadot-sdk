use sp_std::marker::PhantomData;

use cumulus_primitives_core::{AbridgedHrmpChannel, ParaId, PersistedValidationData};
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::{
	dispatch::{DispatchResult, RawOrigin, UnfilteredDispatchable},
	inherent::{InherentData, ProvideInherent},
	traits::{GenesisBuild, OriginTrait},
	weights::Weight,
};
use parachains_common::AccountId;
use polkadot_parachain::primitives::{HrmpChannelId, RelayChainBlockNumber};
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_core::Encode;
use sp_runtime::{Digest, DigestItem};
use xcm::{
	latest::{MultiAsset, MultiLocation, XcmContext, XcmHash},
	prelude::*,
};
use xcm_executor::{traits::TransactAsset, Assets};

pub mod test_cases;
pub use test_cases::CollatorSessionKeys;

pub type BalanceOf<Runtime> = <Runtime as pallet_balances::Config>::Balance;
pub type AccountIdOf<Runtime> = <Runtime as frame_system::Config>::AccountId;
pub type ValidatorIdOf<Runtime> = <Runtime as pallet_session::Config>::ValidatorId;
pub type SessionKeysOf<Runtime> = <Runtime as pallet_session::Config>::Keys;

// Basic builder based on balances, collators and pallet_sessopm
pub struct ExtBuilder<
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config,
> {
	// endowed accounts with balances
	balances: Vec<(AccountIdOf<Runtime>, BalanceOf<Runtime>)>,
	// collators to test block prod
	collators: Vec<AccountIdOf<Runtime>>,
	// keys added to pallet session
	keys: Vec<(AccountIdOf<Runtime>, ValidatorIdOf<Runtime>, SessionKeysOf<Runtime>)>,
	// safe xcm version for pallet_xcm
	safe_xcm_version: Option<XcmVersion>,
	// para id
	para_id: Option<ParaId>,
	_runtime: PhantomData<Runtime>,
}

impl<
		Runtime: frame_system::Config
			+ pallet_balances::Config
			+ pallet_session::Config
			+ pallet_xcm::Config
			+ parachain_info::Config,
	> Default for ExtBuilder<Runtime>
{
	fn default() -> ExtBuilder<Runtime> {
		ExtBuilder {
			balances: vec![],
			collators: vec![],
			keys: vec![],
			safe_xcm_version: None,
			para_id: None,
			_runtime: PhantomData,
		}
	}
}

impl<
		Runtime: frame_system::Config
			+ pallet_balances::Config
			+ pallet_session::Config
			+ pallet_xcm::Config
			+ parachain_info::Config,
	> ExtBuilder<Runtime>
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

	pub fn with_tracing(self) -> Self {
		frame_support::sp_tracing::try_init_simple();
		self
	}

	pub fn with_safe_xcm_version(mut self, safe_xcm_version: XcmVersion) -> Self {
		self.safe_xcm_version = Some(safe_xcm_version);
		self
	}

	pub fn with_para_id(mut self, para_id: ParaId) -> Self {
		self.para_id = Some(para_id);
		self
	}

	pub fn build(self) -> sp_io::TestExternalities
	where
		Runtime:
			pallet_collator_selection::Config + pallet_balances::Config + pallet_session::Config,
		ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	{
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

		<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&pallet_xcm::GenesisConfig { safe_xcm_version: self.safe_xcm_version },
			&mut t,
		)
		.unwrap();

		if let Some(para_id) = self.para_id {
			<parachain_info::GenesisConfig as frame_support::traits::GenesisBuild<Runtime>>::assimilate_storage(
				&parachain_info::GenesisConfig { parachain_id: para_id },
				&mut t,
			)
				.unwrap();
		}

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

impl<XcmConfig: xcm_executor::Config> RuntimeHelper<XcmConfig> {
	pub fn do_transfer(
		from: MultiLocation,
		to: MultiLocation,
		(asset, amount): (MultiLocation, u128),
	) -> Result<Assets, XcmError> {
		<XcmConfig::AssetTransactor as TransactAsset>::transfer_asset(
			&MultiAsset { id: Concrete(asset), fun: Fungible(amount) },
			&from,
			&to,
			// We aren't able to track the XCM that initiated the fee deposit, so we create a
			// fake message hash here
			&XcmContext::with_message_hash([0; 32]),
		)
	}
}

impl<Runtime: pallet_xcm::Config + cumulus_pallet_parachain_system::Config> RuntimeHelper<Runtime> {
	pub fn do_teleport_assets<HrmpChannelOpener>(
		origin: <Runtime as frame_system::Config>::RuntimeOrigin,
		dest: MultiLocation,
		beneficiary: MultiLocation,
		(asset, amount): (MultiLocation, u128),
		open_hrmp_channel: Option<(u32, u32)>,
	) -> DispatchResult
	where
		HrmpChannelOpener: frame_support::inherent::ProvideInherent<
			Call = cumulus_pallet_parachain_system::Call<Runtime>,
		>,
	{
		// open hrmp (if needed)
		if let Some((source_para_id, target_para_id)) = open_hrmp_channel {
			mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(
				source_para_id.into(),
				target_para_id.into(),
			);
		}

		// do teleport
		<pallet_xcm::Pallet<Runtime>>::teleport_assets(
			origin,
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new((Concrete(asset), amount).into()),
			0,
		)
	}
}

impl<Runtime: cumulus_pallet_dmp_queue::Config + cumulus_pallet_parachain_system::Config>
	RuntimeHelper<Runtime>
{
	pub fn execute_as_governance(call: Vec<u8>, require_weight_at_most: Weight) -> Outcome {
		// prepare xcm as governance will do
		let xcm = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Superuser,
				require_weight_at_most,
				call: call.into(),
			},
		]);

		// execute xcm as parent origin
		let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		<<Runtime as cumulus_pallet_dmp_queue::Config>::XcmExecutor>::execute_xcm(
			MultiLocation::parent(),
			xcm,
			hash,
			Self::xcm_max_weight(XcmReceivedFrom::Parent),
		)
	}
}

pub enum XcmReceivedFrom {
	Parent,
	Sibling,
}

impl<ParachainSystem: cumulus_pallet_parachain_system::Config> RuntimeHelper<ParachainSystem> {
	pub fn xcm_max_weight(from: XcmReceivedFrom) -> Weight {
		use frame_support::traits::Get;
		match from {
			XcmReceivedFrom::Parent => ParachainSystem::ReservedDmpWeight::get(),
			XcmReceivedFrom::Sibling => ParachainSystem::ReservedXcmpWeight::get(),
		}
	}
}

impl<Runtime: frame_system::Config + pallet_xcm::Config> RuntimeHelper<Runtime> {
	pub fn assert_pallet_xcm_event_outcome(
		unwrap_pallet_xcm_event: &Box<dyn Fn(Vec<u8>) -> Option<pallet_xcm::Event<Runtime>>>,
		assert_outcome: fn(Outcome),
	) {
		let outcome = <frame_system::Pallet<Runtime>>::events()
			.into_iter()
			.filter_map(|e| unwrap_pallet_xcm_event(e.event.encode()))
			.find_map(|e| match e {
				pallet_xcm::Event::Attempted(outcome) => Some(outcome),
				_ => None,
			});
		match outcome {
			Some(outcome) => assert_outcome(outcome),
			None => assert!(false, "No `pallet_xcm::Event::Attempted(outcome)` event found!"),
		}
	}
}

impl<Runtime: frame_system::Config + cumulus_pallet_xcmp_queue::Config> RuntimeHelper<Runtime> {
	pub fn xcmp_queue_message_sent(
		unwrap_xcmp_queue_event: Box<
			dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
		>,
	) -> Option<XcmHash> {
		<frame_system::Pallet<Runtime>>::events()
			.into_iter()
			.filter_map(|e| unwrap_xcmp_queue_event(e.event.encode()))
			.find_map(|e| match e {
				cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { message_hash } => message_hash,
				_ => None,
			})
	}
}

pub fn assert_metadata<Fungibles, AccountId>(
	asset_id: impl Into<Fungibles::AssetId> + Copy,
	expected_name: &str,
	expected_symbol: &str,
	expected_decimals: u8,
) where
	Fungibles: frame_support::traits::tokens::fungibles::metadata::Inspect<AccountId>
		+ frame_support::traits::tokens::fungibles::Inspect<AccountId>,
{
	assert_eq!(Fungibles::name(asset_id.into()), Vec::from(expected_name),);
	assert_eq!(Fungibles::symbol(asset_id.into()), Vec::from(expected_symbol),);
	assert_eq!(Fungibles::decimals(asset_id.into()), expected_decimals);
}

pub fn assert_total<Fungibles, AccountId>(
	asset_id: impl Into<Fungibles::AssetId> + Copy,
	expected_total_issuance: impl Into<Fungibles::Balance>,
	expected_active_issuance: impl Into<Fungibles::Balance>,
) where
	Fungibles: frame_support::traits::tokens::fungibles::metadata::Inspect<AccountId>
		+ frame_support::traits::tokens::fungibles::Inspect<AccountId>,
{
	assert_eq!(Fungibles::total_issuance(asset_id.into()), expected_total_issuance.into());
	assert_eq!(Fungibles::active_issuance(asset_id.into()), expected_active_issuance.into());
}

/// Helper function which emulates opening HRMP channel which is needed for `XcmpQueue` to pass
pub fn mock_open_hrmp_channel<
	C: cumulus_pallet_parachain_system::Config,
	T: ProvideInherent<Call = cumulus_pallet_parachain_system::Call<C>>,
>(
	sender: ParaId,
	recipient: ParaId,
) {
	let n = 1_u32;
	let mut sproof_builder = RelayStateSproofBuilder::default();
	sproof_builder.para_id = sender;
	sproof_builder.hrmp_channels.insert(
		HrmpChannelId { sender, recipient },
		AbridgedHrmpChannel {
			max_capacity: 10,
			max_total_size: 10_000_000_u32,
			max_message_size: 10_000_000_u32,
			msg_count: 0,
			total_size: 0_u32,
			mqc_head: None,
		},
	);
	sproof_builder.hrmp_egress_channel_index = Some(vec![recipient]);

	let (relay_parent_storage_root, relay_chain_state) = sproof_builder.into_state_root_and_proof();
	let vfp = PersistedValidationData {
		relay_parent_number: n as RelayChainBlockNumber,
		relay_parent_storage_root,
		..Default::default()
	};
	// It is insufficient to push the validation function params
	// to storage; they must also be included in the inherent data.
	let inherent_data = {
		let mut inherent_data = InherentData::default();
		let system_inherent_data = ParachainInherentData {
			validation_data: vfp.clone(),
			relay_chain_state,
			downward_messages: Default::default(),
			horizontal_messages: Default::default(),
		};
		inherent_data
			.put_data(
				cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER,
				&system_inherent_data,
			)
			.expect("failed to put VFP inherent");
		inherent_data
	};

	// execute the block
	T::create_inherent(&inherent_data)
		.expect("got an inherent")
		.dispatch_bypass_filter(RawOrigin::None.into())
		.expect("dispatch succeeded");
}
