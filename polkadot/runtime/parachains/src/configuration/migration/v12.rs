// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A module that is responsible for migration of storage.

use crate::configuration::{self, migration::v11::V11HostConfiguration, Config, Pallet};
use alloc::vec::Vec;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{Defensive, UncheckedOnRuntimeUpgrade},
};
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_primitives::{
	ApprovalVotingParams, AsyncBackingParams, Balance, ExecutorParams, NodeFeatures,
	LEGACY_MIN_BACKING_VOTES, MAX_CODE_SIZE, ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
};
use sp_arithmetic::Perbill;
use sp_core::Get;
use sp_staking::SessionIndex;

#[derive(Clone, Encode, PartialEq, Decode, Debug)]
pub struct V12SchedulerParams<BlockNumber> {
	pub group_rotation_frequency: BlockNumber,
	pub paras_availability_period: BlockNumber,
	pub max_validators_per_core: Option<u32>,
	pub lookahead: u32,
	pub num_cores: u32,
	pub max_availability_timeouts: u32,
	pub on_demand_queue_max_size: u32,
	pub on_demand_target_queue_utilization: Perbill,
	pub on_demand_fee_variability: Perbill,
	pub on_demand_base_fee: Balance,
	pub ttl: BlockNumber,
}

impl<BlockNumber: Default + From<u32>> Default for V12SchedulerParams<BlockNumber> {
	fn default() -> Self {
		Self {
			group_rotation_frequency: 1u32.into(),
			paras_availability_period: 1u32.into(),
			max_validators_per_core: Default::default(),
			lookahead: 1,
			num_cores: Default::default(),
			max_availability_timeouts: Default::default(),
			on_demand_queue_max_size: ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
			on_demand_target_queue_utilization: Perbill::from_percent(25),
			on_demand_fee_variability: Perbill::from_percent(3),
			on_demand_base_fee: 10_000_000u128,
			ttl: 5u32.into(),
		}
	}
}

#[derive(Clone, Encode, PartialEq, Decode, Debug)]
pub struct V12HostConfiguration<BlockNumber> {
	pub max_code_size: u32,
	pub max_head_data_size: u32,
	pub max_upward_queue_count: u32,
	pub max_upward_queue_size: u32,
	pub max_upward_message_size: u32,
	pub max_upward_message_num_per_candidate: u32,
	pub hrmp_max_message_num_per_candidate: u32,
	pub validation_upgrade_cooldown: BlockNumber,
	pub validation_upgrade_delay: BlockNumber,
	pub async_backing_params: AsyncBackingParams,
	pub max_pov_size: u32,
	pub max_downward_message_size: u32,
	pub hrmp_max_parachain_outbound_channels: u32,
	pub hrmp_sender_deposit: Balance,
	pub hrmp_recipient_deposit: Balance,
	pub hrmp_channel_max_capacity: u32,
	pub hrmp_channel_max_total_size: u32,
	pub hrmp_max_parachain_inbound_channels: u32,
	pub hrmp_channel_max_message_size: u32,
	pub executor_params: ExecutorParams,
	pub code_retention_period: BlockNumber,
	pub max_validators: Option<u32>,
	pub dispute_period: SessionIndex,
	pub dispute_post_conclusion_acceptance_period: BlockNumber,
	pub no_show_slots: u32,
	pub n_delay_tranches: u32,
	pub zeroth_delay_tranche_width: u32,
	pub needed_approvals: u32,
	pub relay_vrf_modulo_samples: u32,
	pub pvf_voting_ttl: SessionIndex,
	pub minimum_validation_upgrade_delay: BlockNumber,
	pub minimum_backing_votes: u32,
	pub node_features: NodeFeatures,
	pub approval_voting_params: ApprovalVotingParams,
	pub scheduler_params: V12SchedulerParams<BlockNumber>,
}

impl<BlockNumber: Default + From<u32>> Default for V12HostConfiguration<BlockNumber> {
	fn default() -> Self {
		Self {
			async_backing_params: AsyncBackingParams {
				max_candidate_depth: 0,
				allowed_ancestry_len: 0,
			},
			no_show_slots: 1u32.into(),
			validation_upgrade_cooldown: Default::default(),
			validation_upgrade_delay: 2u32.into(),
			code_retention_period: Default::default(),
			max_code_size: MAX_CODE_SIZE,
			max_pov_size: Default::default(),
			max_head_data_size: Default::default(),
			max_validators: None,
			dispute_period: 6,
			dispute_post_conclusion_acceptance_period: 100.into(),
			n_delay_tranches: 1,
			zeroth_delay_tranche_width: Default::default(),
			needed_approvals: Default::default(),
			relay_vrf_modulo_samples: Default::default(),
			max_upward_queue_count: Default::default(),
			max_upward_queue_size: Default::default(),
			max_downward_message_size: Default::default(),
			max_upward_message_size: Default::default(),
			max_upward_message_num_per_candidate: Default::default(),
			hrmp_sender_deposit: Default::default(),
			hrmp_recipient_deposit: Default::default(),
			hrmp_channel_max_capacity: Default::default(),
			hrmp_channel_max_total_size: Default::default(),
			hrmp_max_parachain_inbound_channels: Default::default(),
			hrmp_channel_max_message_size: Default::default(),
			hrmp_max_parachain_outbound_channels: Default::default(),
			hrmp_max_message_num_per_candidate: Default::default(),
			pvf_voting_ttl: 2u32.into(),
			minimum_validation_upgrade_delay: 2.into(),
			executor_params: Default::default(),
			approval_voting_params: ApprovalVotingParams { max_approval_coalesce_count: 1 },
			minimum_backing_votes: LEGACY_MIN_BACKING_VOTES,
			node_features: NodeFeatures::EMPTY,
			scheduler_params: Default::default(),
		}
	}
}

mod v11 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type ActiveConfig<T: Config> =
		StorageValue<Pallet<T>, V11HostConfiguration<BlockNumberFor<T>>, OptionQuery>;

	#[frame_support::storage_alias]
	pub(crate) type PendingConfigs<T: Config> = StorageValue<
		Pallet<T>,
		Vec<(SessionIndex, V11HostConfiguration<BlockNumberFor<T>>)>,
		OptionQuery,
	>;
}

mod v12 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type ActiveConfig<T: Config> =
		StorageValue<Pallet<T>, V12HostConfiguration<BlockNumberFor<T>>, OptionQuery>;

	#[frame_support::storage_alias]
	pub(crate) type PendingConfigs<T: Config> = StorageValue<
		Pallet<T>,
		Vec<(SessionIndex, V12HostConfiguration<BlockNumberFor<T>>)>,
		OptionQuery,
	>;
}

pub type MigrateToV12<T> = VersionedMigration<
	11,
	12,
	UncheckedMigrateToV12<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

pub struct UncheckedMigrateToV12<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV12<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running pre_upgrade() for HostConfiguration MigrateToV12");
		Ok(Vec::new())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!(target: configuration::LOG_TARGET, "HostConfiguration MigrateToV12 started");
		let weight_consumed = migrate_to_v12::<T>();

		log::info!(target: configuration::LOG_TARGET, "HostConfiguration MigrateToV12 executed successfully");

		weight_consumed
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running post_upgrade() for HostConfiguration MigrateToV12");
		ensure!(
			StorageVersion::get::<Pallet<T>>() >= 12,
			"Storage version should be >= 12 after the migration"
		);

		Ok(())
	}
}

fn migrate_to_v12<T: Config>() -> Weight {
	// Unusual formatting is justified:
	// - make it easier to verify that fields assign what they supposed to assign.
	// - this code is transient and will be removed after all migrations are done.
	// - this code is important enough to optimize for legibility sacrificing consistency.
	#[rustfmt::skip]
		let translate =
		|pre: V11HostConfiguration<BlockNumberFor<T>>| ->
		V12HostConfiguration<BlockNumberFor<T>>
			{
				V12HostConfiguration {
					max_code_size                            : pre.max_code_size,
					max_head_data_size                       : pre.max_head_data_size,
					max_upward_queue_count                   : pre.max_upward_queue_count,
					max_upward_queue_size                    : pre.max_upward_queue_size,
					max_upward_message_size                  : pre.max_upward_message_size,
					max_upward_message_num_per_candidate     : pre.max_upward_message_num_per_candidate,
					hrmp_max_message_num_per_candidate       : pre.hrmp_max_message_num_per_candidate,
					validation_upgrade_cooldown              : pre.validation_upgrade_cooldown,
					validation_upgrade_delay                 : pre.validation_upgrade_delay,
					max_pov_size                             : pre.max_pov_size,
					max_downward_message_size                : pre.max_downward_message_size,
					hrmp_sender_deposit                      : pre.hrmp_sender_deposit,
					hrmp_recipient_deposit                   : pre.hrmp_recipient_deposit,
					hrmp_channel_max_capacity                : pre.hrmp_channel_max_capacity,
					hrmp_channel_max_total_size              : pre.hrmp_channel_max_total_size,
					hrmp_max_parachain_inbound_channels      : pre.hrmp_max_parachain_inbound_channels,
					hrmp_max_parachain_outbound_channels     : pre.hrmp_max_parachain_outbound_channels,
					hrmp_channel_max_message_size            : pre.hrmp_channel_max_message_size,
					code_retention_period                    : pre.code_retention_period,
					max_validators                           : pre.max_validators,
					dispute_period                           : pre.dispute_period,
					dispute_post_conclusion_acceptance_period: pre.dispute_post_conclusion_acceptance_period,
					no_show_slots                            : pre.no_show_slots,
					n_delay_tranches                         : pre.n_delay_tranches,
					zeroth_delay_tranche_width               : pre.zeroth_delay_tranche_width,
					needed_approvals                         : pre.needed_approvals,
					relay_vrf_modulo_samples                 : pre.relay_vrf_modulo_samples,
					pvf_voting_ttl                           : pre.pvf_voting_ttl,
					minimum_validation_upgrade_delay         : pre.minimum_validation_upgrade_delay,
					async_backing_params                     : pre.async_backing_params,
					executor_params                          : pre.executor_params,
					minimum_backing_votes                    : pre.minimum_backing_votes,
					node_features                            : pre.node_features,
					approval_voting_params                   : pre.approval_voting_params,
					scheduler_params: V12SchedulerParams {
							group_rotation_frequency             : pre.group_rotation_frequency,
							paras_availability_period            : pre.paras_availability_period,
							max_validators_per_core              : pre.max_validators_per_core,
							lookahead                            : pre.scheduling_lookahead,
							num_cores                            : pre.coretime_cores,
							max_availability_timeouts            : pre.on_demand_retries,
							on_demand_queue_max_size             : pre.on_demand_queue_max_size,
							on_demand_target_queue_utilization   : pre.on_demand_target_queue_utilization,
							on_demand_fee_variability            : pre.on_demand_fee_variability,
							on_demand_base_fee                   : pre.on_demand_base_fee,
							ttl                                  : pre.on_demand_ttl,
					}
				}
			};

	let v11 = v11::ActiveConfig::<T>::get()
		.defensive_proof("Could not decode old config")
		.unwrap_or_default();
	let v12 = translate(v11);
	v12::ActiveConfig::<T>::set(Some(v12));

	// Allowed to be empty.
	let pending_v11 = v11::PendingConfigs::<T>::get().unwrap_or_default();
	let mut pending_v12 = Vec::new();

	for (session, v11) in pending_v11.into_iter() {
		let v12 = translate(v11);
		pending_v12.push((session, v12));
	}
	v12::PendingConfigs::<T>::set(Some(pending_v12.clone()));

	let num_configs = (pending_v12.len() + 1) as u64;
	T::DbWeight::get().reads_writes(num_configs, num_configs)
}

#[cfg(test)]
mod tests {
	use polkadot_primitives::LEGACY_MIN_BACKING_VOTES;
	use sp_arithmetic::Perbill;

	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	fn v12_deserialized_from_actual_data() {
		// Example how to get new `raw_config`:
		// We'll obtain the raw_config at a specified a block
		// Steps:
		// 1. Go to Polkadot.js -> Developer -> Chain state -> Storage: https://polkadot.js.org/apps/#/chainstate
		// 2. Set these parameters:
		//   2.1. selected state query: configuration; activeConfig():
		//        PolkadotRuntimeParachainsConfigurationHostConfiguration
		//   2.2. blockhash to query at:
		//        0xf89d3ab5312c5f70d396dc59612f0aa65806c798346f9db4b35278baed2e0e53 (the hash of
		//        the block)
		//   2.3. Note the value of encoded storage key ->
		//        0x06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385 for the
		//        referenced block.
		//   2.4. You'll also need the decoded values to update the test.
		// 3. Go to Polkadot.js -> Developer -> Chain state -> Raw storage
		//   3.1 Enter the encoded storage key and you get the raw config.

		// This exceeds the maximal line width length, but that's fine, since this is not code and
		// doesn't need to be read and also leaving it as one line allows to easily copy it.
		let raw_config =
	hex_literal::hex![
	"0000300000800000080000000000100000c8000005000000050000000200000002000000000000000000000000005000000010000400000000000000000000000000000000000000000000000000000000000000000000000800000000200000040000000000100000b004000000060000006400000002000000190000000000000002000000020000000200000005000000020000000001000000140000000400000001010000000100000001000000000000001027000080b2e60e80c3c9018096980000000000000000000000000005000000"
	];

		let v12 =
			V12HostConfiguration::<polkadot_primitives::BlockNumber>::decode(&mut &raw_config[..])
				.unwrap();

		// We check only a sample of the values here. If we missed any fields or messed up data
		// types that would skew all the fields coming after.
		assert_eq!(v12.max_code_size, 3_145_728);
		assert_eq!(v12.validation_upgrade_cooldown, 2);
		assert_eq!(v12.max_pov_size, 5_242_880);
		assert_eq!(v12.hrmp_channel_max_message_size, 1_048_576);
		assert_eq!(v12.n_delay_tranches, 25);
		assert_eq!(v12.minimum_validation_upgrade_delay, 5);
		assert_eq!(v12.minimum_backing_votes, LEGACY_MIN_BACKING_VOTES);
		assert_eq!(v12.approval_voting_params.max_approval_coalesce_count, 1);
		assert_eq!(v12.scheduler_params.group_rotation_frequency, 20);
		assert_eq!(v12.scheduler_params.paras_availability_period, 4);
		assert_eq!(v12.scheduler_params.lookahead, 1);
		assert_eq!(v12.scheduler_params.num_cores, 1);
		assert_eq!(v12.scheduler_params.max_availability_timeouts, 0);
		assert_eq!(v12.scheduler_params.on_demand_queue_max_size, 10_000);
		assert_eq!(
			v12.scheduler_params.on_demand_target_queue_utilization,
			Perbill::from_percent(25)
		);
		assert_eq!(v12.scheduler_params.on_demand_fee_variability, Perbill::from_percent(3));
		assert_eq!(v12.scheduler_params.on_demand_base_fee, 10_000_000);
		assert_eq!(v12.scheduler_params.ttl, 5);
	}

	#[test]
	fn test_migrate_to_v12() {
		// Host configuration has lots of fields. However, in this migration we only add one
		// field. The most important part to check are a couple of the last fields. We also pick
		// extra fields to check arbitrarily, e.g. depending on their position (i.e. the middle) and
		// also their type.
		//
		// We specify only the picked fields and the rest should be provided by the `Default`
		// implementation. That implementation is copied over between the two types and should work
		// fine.
		let v11 = V11HostConfiguration::<polkadot_primitives::BlockNumber> {
			needed_approvals: 69,
			paras_availability_period: 55,
			hrmp_recipient_deposit: 1337,
			max_pov_size: 1111,
			minimum_validation_upgrade_delay: 20,
			on_demand_ttl: 3,
			on_demand_retries: 10,
			..Default::default()
		};

		let mut pending_configs = Vec::new();
		pending_configs.push((100, v11.clone()));
		pending_configs.push((300, v11.clone()));

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v10 version in the state.
			v11::ActiveConfig::<Test>::set(Some(v11.clone()));
			v11::PendingConfigs::<Test>::set(Some(pending_configs));

			migrate_to_v12::<Test>();

			let v12 = v12::ActiveConfig::<Test>::get().unwrap();
			assert_eq!(v12.approval_voting_params.max_approval_coalesce_count, 1);

			let mut configs_to_check = v12::PendingConfigs::<Test>::get().unwrap();
			configs_to_check.push((0, v12.clone()));

			for (_, v12) in configs_to_check {
				#[rustfmt::skip]
				{
					assert_eq!(v11.max_code_size                            , v12.max_code_size);
					assert_eq!(v11.max_head_data_size                       , v12.max_head_data_size);
					assert_eq!(v11.max_upward_queue_count                   , v12.max_upward_queue_count);
					assert_eq!(v11.max_upward_queue_size                    , v12.max_upward_queue_size);
					assert_eq!(v11.max_upward_message_size                  , v12.max_upward_message_size);
					assert_eq!(v11.max_upward_message_num_per_candidate     , v12.max_upward_message_num_per_candidate);
					assert_eq!(v11.hrmp_max_message_num_per_candidate       , v12.hrmp_max_message_num_per_candidate);
					assert_eq!(v11.validation_upgrade_cooldown              , v12.validation_upgrade_cooldown);
					assert_eq!(v11.validation_upgrade_delay                 , v12.validation_upgrade_delay);
					assert_eq!(v11.max_pov_size                             , v12.max_pov_size);
					assert_eq!(v11.max_downward_message_size                , v12.max_downward_message_size);
					assert_eq!(v11.hrmp_max_parachain_outbound_channels     , v12.hrmp_max_parachain_outbound_channels);
					assert_eq!(v11.hrmp_sender_deposit                      , v12.hrmp_sender_deposit);
					assert_eq!(v11.hrmp_recipient_deposit                   , v12.hrmp_recipient_deposit);
					assert_eq!(v11.hrmp_channel_max_capacity                , v12.hrmp_channel_max_capacity);
					assert_eq!(v11.hrmp_channel_max_total_size              , v12.hrmp_channel_max_total_size);
					assert_eq!(v11.hrmp_max_parachain_inbound_channels      , v12.hrmp_max_parachain_inbound_channels);
					assert_eq!(v11.hrmp_channel_max_message_size            , v12.hrmp_channel_max_message_size);
					assert_eq!(v11.code_retention_period                    , v12.code_retention_period);
					assert_eq!(v11.max_validators                           , v12.max_validators);
					assert_eq!(v11.dispute_period                           , v12.dispute_period);
					assert_eq!(v11.no_show_slots                            , v12.no_show_slots);
					assert_eq!(v11.n_delay_tranches                         , v12.n_delay_tranches);
					assert_eq!(v11.zeroth_delay_tranche_width               , v12.zeroth_delay_tranche_width);
					assert_eq!(v11.needed_approvals                         , v12.needed_approvals);
					assert_eq!(v11.relay_vrf_modulo_samples                 , v12.relay_vrf_modulo_samples);
					assert_eq!(v11.pvf_voting_ttl                           , v12.pvf_voting_ttl);
					assert_eq!(v11.minimum_validation_upgrade_delay         , v12.minimum_validation_upgrade_delay);
					assert_eq!(v11.async_backing_params.allowed_ancestry_len, v12.async_backing_params.allowed_ancestry_len);
					assert_eq!(v11.async_backing_params.max_candidate_depth , v12.async_backing_params.max_candidate_depth);
					assert_eq!(v11.executor_params                          , v12.executor_params);
				    assert_eq!(v11.minimum_backing_votes                    , v12.minimum_backing_votes);
					assert_eq!(v11.group_rotation_frequency                 , v12.scheduler_params.group_rotation_frequency);
					assert_eq!(v11.paras_availability_period                , v12.scheduler_params.paras_availability_period);
					assert_eq!(v11.max_validators_per_core                  , v12.scheduler_params.max_validators_per_core);
					assert_eq!(v11.scheduling_lookahead                     , v12.scheduler_params.lookahead);
					assert_eq!(v11.coretime_cores                           , v12.scheduler_params.num_cores);
					assert_eq!(v11.on_demand_retries                        , v12.scheduler_params.max_availability_timeouts);
					assert_eq!(v11.on_demand_queue_max_size                 , v12.scheduler_params.on_demand_queue_max_size);
					assert_eq!(v11.on_demand_target_queue_utilization       , v12.scheduler_params.on_demand_target_queue_utilization);
					assert_eq!(v11.on_demand_fee_variability                , v12.scheduler_params.on_demand_fee_variability);
					assert_eq!(v11.on_demand_base_fee                       , v12.scheduler_params.on_demand_base_fee);
					assert_eq!(v11.on_demand_ttl                            , v12.scheduler_params.ttl);
				}; // ; makes this a statement. `rustfmt::skip` cannot be put on an expression.
			}
		});
	}

	// Test that migration doesn't panic in case there are no pending configurations upgrades in
	// pallet's storage.
	#[test]
	fn test_migrate_to_v12_no_pending() {
		let v11 = V11HostConfiguration::<polkadot_primitives::BlockNumber>::default();

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v10 version in the state.
			v11::ActiveConfig::<Test>::set(Some(v11));
			// Ensure there are no pending configs.
			v12::PendingConfigs::<Test>::set(None);

			// Shouldn't fail.
			migrate_to_v12::<Test>();
		});
	}
}
