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
	AccountId, Babe, Balance, Balances, BlockNumber, Registrar, Runtime, RuntimeCall, RuntimeEvent,
	RuntimeOrigin, ShiftSessionManager, Slots, UncheckedExtrinsic,
};

use frame_support::{parameter_types, traits::KeyOwnerProofSystem};
use frame_system::EnsureRoot;
use polkadot_primitives::v4::{ValidatorId, ValidatorIndex};
use polkadot_runtime_common::{paras_registrar, paras_sudo_wrapper, slots};
use polkadot_runtime_parachains::{
	configuration as parachains_configuration, disputes as parachains_disputes,
	disputes::slashing as parachains_slashing, dmp as parachains_dmp, hrmp as parachains_hrmp,
	inclusion as parachains_inclusion, initializer as parachains_initializer,
	origin as parachains_origin, paras as parachains_paras,
	paras_inherent as parachains_paras_inherent, scheduler as parachains_scheduler,
	session_info as parachains_session_info, shared as parachains_shared, ump as parachains_ump,
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::transaction_validity::TransactionPriority;

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
	type Currency = Balances;
	type WeightInfo = parachains_hrmp::TestWeightInfo;
}

impl parachains_inclusion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardValidators = RewardValidators;
	type DisputesHandler = ();
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
	type WeightInfo = parachains_paras::TestWeightInfo;
	type UnsignedPriority = ParasUnsignedPriority;
	type NextSessionRotation = Babe;
}

impl parachains_paras_inherent::Config for Runtime {
	type WeightInfo = parachains_paras_inherent::TestWeightInfo;
}

impl parachains_scheduler::Config for Runtime {}

impl parachains_session_info::Config for Runtime {
	type ValidatorSet = ShiftSessionManager;
}

impl parachains_shared::Config for Runtime {}

impl parachains_ump::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type UmpSink = ();
	type FirstMessageFactorPercent = frame_support::traits::ConstU64<100>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
	type WeightInfo = parachains_ump::TestWeightInfo;
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
