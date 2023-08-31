// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use sp_std::marker::PhantomData;

use codec::{Decode, DecodeLimit};
use cumulus_primitives_core::{
	relay_chain::Slot, AbridgedHrmpChannel, ParaId, PersistedValidationData,
};
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::{
	dispatch::{DispatchResult, RawOrigin},
	inherent::{InherentData, ProvideInherent},
	traits::{OnFinalize, OnInitialize, OriginTrait, UnfilteredDispatchable},
	weights::Weight,
};
use frame_system::pallet_prelude::{BlockNumberFor, HeaderFor};
use parachains_common::{AccountId, SLOT_DURATION};
use polkadot_parachain_primitives::primitives::{
	HeadData, HrmpChannelId, RelayChainBlockNumber, XcmpMessageFormat,
};
use sp_consensus_aura::{SlotDuration, AURA_ENGINE_ID};
use sp_core::Encode;
use sp_runtime::{traits::Header, BuildStorage, Digest, DigestItem};
use xcm::{
	latest::{MultiAsset, MultiLocation, XcmContext, XcmHash},
	prelude::*,
	VersionedXcm, MAX_XCM_DECODE_DEPTH,
};
use xcm_executor::{traits::TransactAsset, Assets};

pub mod test_cases;

pub type BalanceOf<Runtime> = <Runtime as pallet_balances::Config>::Balance;
pub type AccountIdOf<Runtime> = <Runtime as frame_system::Config>::AccountId;
pub type ValidatorIdOf<Runtime> = <Runtime as pallet_session::Config>::ValidatorId;
pub type SessionKeysOf<Runtime> = <Runtime as pallet_session::Config>::Keys;

pub struct CollatorSessionKeys<
	Runtime: frame_system::Config + pallet_balances::Config + pallet_session::Config,
> {
	collator: AccountIdOf<Runtime>,
	validator: ValidatorIdOf<Runtime>,
	key: SessionKeysOf<Runtime>,
}

impl<Runtime: frame_system::Config + pallet_balances::Config + pallet_session::Config>
	CollatorSessionKeys<Runtime>
{
	pub fn new(
		collator: AccountIdOf<Runtime>,
		validator: ValidatorIdOf<Runtime>,
		key: SessionKeysOf<Runtime>,
	) -> Self {
		Self { collator, validator, key }
	}
	pub fn collators(&self) -> Vec<AccountIdOf<Runtime>> {
		vec![self.collator.clone()]
	}

	pub fn session_keys(
		&self,
	) -> Vec<(AccountIdOf<Runtime>, ValidatorIdOf<Runtime>, SessionKeysOf<Runtime>)> {
		vec![(self.collator.clone(), self.validator.clone(), self.key.clone())]
	}
}

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
		sp_tracing::try_init_simple();
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
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		pallet_xcm::GenesisConfig::<Runtime> {
			safe_xcm_version: self.safe_xcm_version,
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		if let Some(para_id) = self.para_id {
			parachain_info::GenesisConfig::<Runtime> {
				parachain_id: para_id,
				..Default::default()
			}
			.assimilate_storage(&mut t)
			.unwrap();
		}

		pallet_balances::GenesisConfig::<Runtime> { balances: self.balances }
			.assimilate_storage(&mut t)
			.unwrap();

		pallet_collator_selection::GenesisConfig::<Runtime> {
			invulnerables: self.collators.clone(),
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

pub struct RuntimeHelper<Runtime, AllPalletsWithoutSystem>(
	PhantomData<(Runtime, AllPalletsWithoutSystem)>,
);
/// Utility function that advances the chain to the desired block number.
/// If an author is provided, that author information is injected to all the blocks in the meantime.
impl<Runtime: frame_system::Config, AllPalletsWithoutSystem>
	RuntimeHelper<Runtime, AllPalletsWithoutSystem>
where
	AccountIdOf<Runtime>:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
{
	pub fn run_to_block(n: u32, author: AccountId) -> HeaderFor<Runtime> {
		let mut last_header = None;
		loop {
			let block_number = frame_system::Pallet::<Runtime>::block_number();
			if block_number >= n.into() {
				break
			}
			// Set the new block number and author

			// Inherent is not created at every block, don't finalize parachain
			// system to avoid panicking.
			let header = frame_system::Pallet::<Runtime>::finalize();

			let pre_digest =
				Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, author.encode())] };
			frame_system::Pallet::<Runtime>::reset_events();

			let next_block_number = block_number + 1u32.into();
			frame_system::Pallet::<Runtime>::initialize(
				&next_block_number,
				&header.hash(),
				&pre_digest,
			);
			AllPalletsWithoutSystem::on_initialize(next_block_number);
			last_header = Some(header);
		}
		last_header.expect("run_to_block empty block range")
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

impl<XcmConfig: xcm_executor::Config, AllPalletsWithoutSystem>
	RuntimeHelper<XcmConfig, AllPalletsWithoutSystem>
{
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
			&XcmContext::with_message_id([0; 32]),
		)
	}
}

impl<
		Runtime: pallet_xcm::Config + cumulus_pallet_parachain_system::Config,
		AllPalletsWithoutSystem,
	> RuntimeHelper<Runtime, AllPalletsWithoutSystem>
{
	pub fn do_teleport_assets<HrmpChannelOpener>(
		origin: <Runtime as frame_system::Config>::RuntimeOrigin,
		dest: MultiLocation,
		beneficiary: MultiLocation,
		(asset, amount): (MultiLocation, u128),
		open_hrmp_channel: Option<(u32, u32)>,
		included_head: HeaderFor<Runtime>,
		slot_digest: &[u8],
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
				included_head,
				slot_digest,
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

impl<
		Runtime: cumulus_pallet_dmp_queue::Config + cumulus_pallet_parachain_system::Config,
		AllPalletsWithoutSystem,
	> RuntimeHelper<Runtime, AllPalletsWithoutSystem>
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

impl<ParachainSystem: cumulus_pallet_parachain_system::Config, AllPalletsWithoutSystem>
	RuntimeHelper<ParachainSystem, AllPalletsWithoutSystem>
{
	pub fn xcm_max_weight(from: XcmReceivedFrom) -> Weight {
		use frame_support::traits::Get;
		match from {
			XcmReceivedFrom::Parent => ParachainSystem::ReservedDmpWeight::get(),
			XcmReceivedFrom::Sibling => ParachainSystem::ReservedXcmpWeight::get(),
		}
	}
}

impl<Runtime: frame_system::Config + pallet_xcm::Config, AllPalletsWithoutSystem>
	RuntimeHelper<Runtime, AllPalletsWithoutSystem>
{
	pub fn assert_pallet_xcm_event_outcome(
		unwrap_pallet_xcm_event: &Box<dyn Fn(Vec<u8>) -> Option<pallet_xcm::Event<Runtime>>>,
		assert_outcome: fn(Outcome),
	) {
		let outcome = <frame_system::Pallet<Runtime>>::events()
			.into_iter()
			.filter_map(|e| unwrap_pallet_xcm_event(e.event.encode()))
			.find_map(|e| match e {
				pallet_xcm::Event::Attempted { outcome } => Some(outcome),
				_ => None,
			})
			.expect("No `pallet_xcm::Event::Attempted(outcome)` event found!");

		assert_outcome(outcome);
	}
}

impl<
		Runtime: frame_system::Config + cumulus_pallet_xcmp_queue::Config,
		AllPalletsWithoutSystem,
	> RuntimeHelper<Runtime, AllPalletsWithoutSystem>
{
	pub fn xcmp_queue_message_sent(
		unwrap_xcmp_queue_event: Box<
			dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
		>,
	) -> Option<XcmHash> {
		<frame_system::Pallet<Runtime>>::events()
			.into_iter()
			.filter_map(|e| unwrap_xcmp_queue_event(e.event.encode()))
			.find_map(|e| match e {
				cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { message_hash } =>
					Some(message_hash),
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

/// Helper function which emulates opening HRMP channel which is needed for `XcmpQueue` to pass.
///
/// Calls parachain-system's `create_inherent` in case the channel hasn't been opened before, and
/// thus requires additional parameters for validating it: latest included parachain head and
/// parachain AuRa-slot.
///
/// AuRa consensus hook expects pallets to be initialized, before calling this function make sure to
/// `run_to_block` at least once.
pub fn mock_open_hrmp_channel<
	C: cumulus_pallet_parachain_system::Config,
	T: ProvideInherent<Call = cumulus_pallet_parachain_system::Call<C>>,
>(
	sender: ParaId,
	recipient: ParaId,
	included_head: HeaderFor<C>,
	mut slot_digest: &[u8],
) {
	const RELAY_CHAIN_SLOT_DURATION: SlotDuration = SlotDuration::from_millis(6000);
	let slot = Slot::decode(&mut slot_digest).expect("failed to decode digest");
	// Convert para slot to relay chain.
	let timestamp = slot.saturating_mul(SLOT_DURATION);
	let relay_slot = Slot::from_timestamp(timestamp.into(), RELAY_CHAIN_SLOT_DURATION);

	let n = 1_u32;
	let mut sproof_builder = RelayStateSproofBuilder {
		para_id: sender,
		included_para_head: Some(HeadData(included_head.encode())),
		hrmp_egress_channel_index: Some(vec![recipient]),
		current_slot: relay_slot,
		..Default::default()
	};
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
			validation_data: vfp,
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

impl<HrmpChannelSource: cumulus_primitives_core::XcmpMessageSource, AllPalletsWithoutSystem>
	RuntimeHelper<HrmpChannelSource, AllPalletsWithoutSystem>
{
	pub fn take_xcm(sent_to_para_id: ParaId) -> Option<VersionedXcm<()>> {
		match HrmpChannelSource::take_outbound_messages(10)[..] {
			[(para_id, ref mut xcm_message_data)] if para_id.eq(&sent_to_para_id.into()) => {
				let mut xcm_message_data = &xcm_message_data[..];
				// decode
				let _ = XcmpMessageFormat::decode_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut xcm_message_data,
				)
				.expect("valid format");
				VersionedXcm::<()>::decode_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut xcm_message_data,
				)
				.map(|x| Some(x))
				.expect("result with xcm")
			},
			_ => return None,
		}
	}
}
