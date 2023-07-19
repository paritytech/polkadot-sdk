// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Parachains support in Rialto runtime.

use crate::{
	xcm_config, AccountId, Babe, Balance, Balances, BlockNumber, Registrar, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeOrigin, ShiftSessionManager, Slots, UncheckedExtrinsic,
};

use frame_support::{
	parameter_types,
	traits::{KeyOwnerProofSystem, ProcessMessage, ProcessMessageError},
	weights::{Weight, WeightMeter},
};
use frame_system::EnsureRoot;
use polkadot_primitives::v5::{ValidatorId, ValidatorIndex};
use polkadot_runtime_common::{paras_registrar, paras_sudo_wrapper, slots};
use polkadot_runtime_parachains::{
	configuration as parachains_configuration, disputes as parachains_disputes,
	disputes::slashing as parachains_slashing,
	dmp as parachains_dmp, hrmp as parachains_hrmp, inclusion as parachains_inclusion,
	inclusion::{AggregateMessageOrigin, UmpQueueId},
	initializer as parachains_initializer, origin as parachains_origin, paras as parachains_paras,
	paras_inherent as parachains_paras_inherent, scheduler as parachains_scheduler,
	session_info as parachains_session_info, shared as parachains_shared,
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::transaction_validity::TransactionPriority;
use xcm::latest::Junction;

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = RuntimeCall;
}

/// Special `RewardValidators` that does nothing ;)
pub struct RewardValidators;
impl polkadot_runtime_parachains::inclusion::RewardValidators for RewardValidators {
	fn reward_backing(_: impl IntoIterator<Item = ValidatorIndex>) {}
	fn reward_bitfields(_: impl IntoIterator<Item = ValidatorIndex>) {}
}

// all required parachain modules from `polkadot-runtime-parachains` crate

impl parachains_configuration::Config for Runtime {
	type WeightInfo = parachains_configuration::TestWeightInfo;
}

impl parachains_dmp::Config for Runtime {}

impl parachains_hrmp::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type ChannelManager = EnsureRoot<Self::AccountId>;
	type Currency = Balances;
	type WeightInfo = parachains_hrmp::TestWeightInfo;
}

impl parachains_inclusion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardValidators = RewardValidators;
	type DisputesHandler = ();
	type MessageQueue = crate::MessageQueue;
	type WeightInfo = ();
}

impl parachains_initializer::Config for Runtime {
	type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type WeightInfo = ();
}

impl parachains_disputes::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardValidators = ();
	type SlashingHandler = ();
	type WeightInfo = parachains_disputes::TestWeightInfo;
}

impl parachains_slashing::Config for Runtime {
	type KeyOwnerProofSystem = ();
	type KeyOwnerProof =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, ValidatorId)>>::Proof;
	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		ValidatorId,
	)>>::IdentificationTuple;
	type HandleReports = ();
	type WeightInfo = parachains_slashing::TestWeightInfo;
	type BenchmarkingConfig = parachains_slashing::BenchConfig<200>;
}

impl parachains_origin::Config for Runtime {}

parameter_types! {
	pub const ParasUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
}

impl parachains_paras::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ParasWeightInfo;
	type UnsignedPriority = ParasUnsignedPriority;
	type QueueFootprinter = crate::Inclusion;
	type NextSessionRotation = Babe;
}

/// Test weight for the `Paras` pallet.
///
/// We can't use `parachains_paras::TestWeightInfo` anymore, because it returns `Weight::MAX`
/// where we need some real-world weights. We'll use zero weights here, though to avoid
/// adding benchmarks to Rialto runtime.
pub struct ParasWeightInfo;

impl parachains_paras::WeightInfo for ParasWeightInfo {
	fn force_set_current_code(_c: u32) -> Weight {
		Weight::zero()
	}
	fn force_set_current_head(_s: u32) -> Weight {
		Weight::zero()
	}
	fn force_schedule_code_upgrade(_c: u32) -> Weight {
		Weight::zero()
	}
	fn force_note_new_head(_s: u32) -> Weight {
		Weight::zero()
	}
	fn force_queue_action() -> Weight {
		Weight::zero()
	}
	fn add_trusted_validation_code(_c: u32) -> Weight {
		Weight::zero()
	}
	fn poke_unused_validation_code() -> Weight {
		Weight::zero()
	}
	fn include_pvf_check_statement_finalize_upgrade_accept() -> Weight {
		Weight::zero()
	}
	fn include_pvf_check_statement_finalize_upgrade_reject() -> Weight {
		Weight::zero()
	}
	fn include_pvf_check_statement_finalize_onboarding_accept() -> Weight {
		Weight::zero()
	}
	fn include_pvf_check_statement_finalize_onboarding_reject() -> Weight {
		Weight::zero()
	}
	fn include_pvf_check_statement() -> Weight {
		Weight::zero()
	}
}

impl parachains_paras_inherent::Config for Runtime {
	type WeightInfo = parachains_paras_inherent::TestWeightInfo;
}

impl parachains_scheduler::Config for Runtime {}

impl parachains_session_info::Config for Runtime {
	type ValidatorSet = ShiftSessionManager;
}

impl parachains_shared::Config for Runtime {}

parameter_types! {
	/// Amount of weight that can be spent per block to service messages.
	///
	/// # WARNING
	///
	/// This is not a good value for para-chains since the `Scheduler`
	/// already uses up to 80 percent block weight.
	pub MessageQueueServiceWeight: Weight = crate::Perbill::from_percent(20) * bp_rialto::BlockWeights::get().max_block;
	pub const MessageQueueHeapSize: u32 = 32 * 1024;
	pub const MessageQueueMaxStale: u32 = 96;
}

/// Message processor to handle any messages that were enqueued into the `MessageQueue` pallet.
pub struct MessageProcessor;
impl ProcessMessage for MessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)) => para,
		};
		xcm_builder::ProcessXcmMessage::<
			Junction,
			xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
			RuntimeCall,
		>::process_message(message, Junction::Parachain(para.into()), meter, id)
	}
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Size = u32;
	type HeapSize = MessageQueueHeapSize;
	type MaxStale = MessageQueueMaxStale;
	type ServiceWeight = MessageQueueServiceWeight;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = MessageProcessor;
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor =
		pallet_message_queue::mock_helpers::NoopMessageProcessor<AggregateMessageOrigin>;
	type QueueChangeHandler = crate::Inclusion;
	type WeightInfo = ();
	type QueuePausedQuery = ();
}

// required onboarding pallets. We're not going to use auctions or crowdloans, so they're missing

parameter_types! {
	pub const ParaDeposit: Balance = 0;
	pub const DataDepositPerByte: Balance = 0;
}

impl paras_registrar::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type Currency = Balances;
	type OnSwap = Slots;
	type ParaDeposit = ParaDeposit;
	type DataDepositPerByte = DataDepositPerByte;
	type WeightInfo = paras_registrar::TestWeightInfo;
}

parameter_types! {
	pub const LeasePeriod: BlockNumber = 10 * bp_rialto::MINUTES;
}

impl slots::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type Registrar = Registrar;
	type LeasePeriod = LeasePeriod;
	type WeightInfo = slots::TestWeightInfo;
	type LeaseOffset = ();
	type ForceOrigin = EnsureRoot<AccountId>;
}

impl paras_sudo_wrapper::Config for Runtime {}
