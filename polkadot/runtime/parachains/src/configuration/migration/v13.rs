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

//! A module that is responsible for migration of storage for the configuration pallet.
//! v12 -> v13:
//! - Added `max_relay_parent_session_age` field to configuration
//! - Removed `ttl` and `max_availability_timeouts` from `SchedulerParams`.

use crate::configuration::{self, Config, Pallet};
use alloc::vec::Vec;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{Defensive, UncheckedOnRuntimeUpgrade},
};
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_core_primitives::Balance;
use polkadot_primitives::vstaging::SchedulerParams;
use sp_core::Get;
use sp_staking::SessionIndex;

type V13HostConfiguration<BlockNumber> = configuration::HostConfiguration<BlockNumber>;

/// The v12 `SchedulerParams`, before the `max_relay_parent_session_age` field was added.
/// This is identical to `polkadot_primitives::v9::SchedulerParams`.
pub type V12SchedulerParams<BlockNumber> = polkadot_primitives::v9::SchedulerParams<BlockNumber>;

/// The v12 `HostConfiguration`, before the `max_relay_parent_session_age` field was added to
/// `SchedulerParams`.
#[derive(Encode, Decode, Debug, Clone)]
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
	pub async_backing_params: polkadot_primitives::AsyncBackingParams,
	pub max_pov_size: u32,
	pub max_downward_message_size: u32,
	pub hrmp_max_parachain_outbound_channels: u32,
	pub hrmp_sender_deposit: Balance,
	pub hrmp_recipient_deposit: Balance,
	pub hrmp_channel_max_capacity: u32,
	pub hrmp_channel_max_total_size: u32,
	pub hrmp_max_parachain_inbound_channels: u32,
	pub hrmp_channel_max_message_size: u32,
	pub executor_params: polkadot_primitives::ExecutorParams,
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
	pub node_features: polkadot_primitives::NodeFeatures,
	pub approval_voting_params: polkadot_primitives::ApprovalVotingParams,
	pub scheduler_params: V12SchedulerParams<BlockNumber>,
}

impl<BlockNumber: Default + From<u32>> Default for V12HostConfiguration<BlockNumber> {
	fn default() -> Self {
		Self {
			async_backing_params: polkadot_primitives::AsyncBackingParams {
				max_candidate_depth: 0,
				allowed_ancestry_len: 0,
			},
			no_show_slots: 1u32.into(),
			validation_upgrade_cooldown: Default::default(),
			validation_upgrade_delay: 2u32.into(),
			code_retention_period: Default::default(),
			max_code_size: polkadot_primitives::MAX_CODE_SIZE,
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
			approval_voting_params: polkadot_primitives::ApprovalVotingParams {
				max_approval_coalesce_count: 1,
			},
			minimum_backing_votes: polkadot_primitives::LEGACY_MIN_BACKING_VOTES,
			node_features: Default::default(),
			scheduler_params: Default::default(),
		}
	}
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

mod v13 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type ActiveConfig<T: Config> =
		StorageValue<Pallet<T>, V13HostConfiguration<BlockNumberFor<T>>, OptionQuery>;

	#[frame_support::storage_alias]
	pub(crate) type PendingConfigs<T: Config> = StorageValue<
		Pallet<T>,
		Vec<(SessionIndex, V13HostConfiguration<BlockNumberFor<T>>)>,
		OptionQuery,
	>;
}

pub type MigrateToV13<T> = VersionedMigration<
	12,
	13,
	UncheckedMigrateToV13<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

pub struct UncheckedMigrateToV13<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV13<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running pre_upgrade() for HostConfiguration MigrateToV13");
		Ok(Vec::new())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!(target: configuration::LOG_TARGET, "HostConfiguration MigrateToV13 started");
		let weight_consumed = migrate_to_v13::<T>();

		log::info!(target: configuration::LOG_TARGET, "HostConfiguration MigrateToV13 executed successfully");

		weight_consumed
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running post_upgrade() for HostConfiguration MigrateToV13");
		ensure!(
			StorageVersion::get::<Pallet<T>>() >= 13,
			"Storage version should be >= 13 after the migration"
		);

		Ok(())
	}
}

fn migrate_to_v13<T: Config>() -> Weight {
	// Unusual formatting is justified:
	// - make it easier to verify that fields assign what they supposed to assign.
	// - this code is transient and will be removed after all migrations are done.
	// - this code is important enough to optimize for legibility sacrificing consistency.
	#[rustfmt::skip]
		let translate =
		|pre: V12HostConfiguration<BlockNumberFor<T>>| ->
		V13HostConfiguration<BlockNumberFor<T>>
			{
				V13HostConfiguration {
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
					scheduler_params: SchedulerParams {
							group_rotation_frequency             : pre.scheduler_params.group_rotation_frequency,
							paras_availability_period            : pre.scheduler_params.paras_availability_period,
							max_validators_per_core              : pre.scheduler_params.max_validators_per_core,
							lookahead                            : pre.scheduler_params.lookahead,
							num_cores                            : pre.scheduler_params.num_cores,
							on_demand_queue_max_size             : pre.scheduler_params.on_demand_queue_max_size,
							on_demand_target_queue_utilization   : pre.scheduler_params.on_demand_target_queue_utilization,
							on_demand_fee_variability            : pre.scheduler_params.on_demand_fee_variability,
							on_demand_base_fee                   : pre.scheduler_params.on_demand_base_fee,
					},
					// New field: default to 0 (only allowing backing of candidates with relay parent in the current session).
					max_relay_parent_session_age              : 0,
				}
			};

	let v12 = v12::ActiveConfig::<T>::get()
		.defensive_proof("Could not decode old config")
		.unwrap_or_default();
	let v13 = translate(v12);
	v13::ActiveConfig::<T>::set(Some(v13));

	// Allowed to be empty.
	let pending_v12 = v12::PendingConfigs::<T>::get().unwrap_or_default();
	let mut pending_v13 = Vec::with_capacity(pending_v12.len());

	for (session, v12) in pending_v12.into_iter() {
		let v13 = translate(v12);
		pending_v13.push((session, v13));
	}
	v13::PendingConfigs::<T>::set(Some(pending_v13.clone()));

	let num_configs = (pending_v13.len() + 1) as u64;
	T::DbWeight::get().reads_writes(num_configs, num_configs)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	fn test_migrate_to_v13() {
		let v12 = V12HostConfiguration::<polkadot_primitives::BlockNumber> {
			scheduler_params: V12SchedulerParams { lookahead: 3, ..Default::default() },
			..Default::default()
		};

		let mut pending_configs = Vec::new();
		pending_configs.push((100, v12.clone()));
		pending_configs.push((300, v12.clone()));

		new_test_ext(Default::default()).execute_with(|| {
			v12::ActiveConfig::<Test>::set(Some(v12.clone()));
			v12::PendingConfigs::<Test>::set(Some(pending_configs));

			migrate_to_v13::<Test>();

			let v13 = v13::ActiveConfig::<Test>::get().unwrap();

			let mut configs_to_check = v13::PendingConfigs::<Test>::get().unwrap();
			configs_to_check.push((0, v13.clone()));

			for (_, v13) in configs_to_check {
				#[rustfmt::skip]
				#[allow(deprecated)]
				{
					assert_eq!(v12.max_code_size                            , v13.max_code_size);
					assert_eq!(v12.max_head_data_size                       , v13.max_head_data_size);
					assert_eq!(v12.max_upward_queue_count                   , v13.max_upward_queue_count);
					assert_eq!(v12.max_upward_queue_size                    , v13.max_upward_queue_size);
					assert_eq!(v12.max_upward_message_size                  , v13.max_upward_message_size);
					assert_eq!(v12.max_upward_message_num_per_candidate     , v13.max_upward_message_num_per_candidate);
					assert_eq!(v12.hrmp_max_message_num_per_candidate       , v13.hrmp_max_message_num_per_candidate);
					assert_eq!(v12.validation_upgrade_cooldown              , v13.validation_upgrade_cooldown);
					assert_eq!(v12.validation_upgrade_delay                 , v13.validation_upgrade_delay);
					assert_eq!(v12.max_pov_size                             , v13.max_pov_size);
					assert_eq!(v12.max_downward_message_size                , v13.max_downward_message_size);
					assert_eq!(v12.hrmp_max_parachain_outbound_channels     , v13.hrmp_max_parachain_outbound_channels);
					assert_eq!(v12.hrmp_sender_deposit                      , v13.hrmp_sender_deposit);
					assert_eq!(v12.hrmp_recipient_deposit                   , v13.hrmp_recipient_deposit);
					assert_eq!(v12.hrmp_channel_max_capacity                , v13.hrmp_channel_max_capacity);
					assert_eq!(v12.hrmp_channel_max_total_size              , v13.hrmp_channel_max_total_size);
					assert_eq!(v12.hrmp_max_parachain_inbound_channels      , v13.hrmp_max_parachain_inbound_channels);
					assert_eq!(v12.hrmp_channel_max_message_size            , v13.hrmp_channel_max_message_size);
					assert_eq!(v12.code_retention_period                    , v13.code_retention_period);
					assert_eq!(v12.max_validators                           , v13.max_validators);
					assert_eq!(v12.dispute_period                           , v13.dispute_period);
					assert_eq!(v12.no_show_slots                            , v13.no_show_slots);
					assert_eq!(v12.n_delay_tranches                         , v13.n_delay_tranches);
					assert_eq!(v12.zeroth_delay_tranche_width               , v13.zeroth_delay_tranche_width);
					assert_eq!(v12.needed_approvals                         , v13.needed_approvals);
					assert_eq!(v12.relay_vrf_modulo_samples                 , v13.relay_vrf_modulo_samples);
					assert_eq!(v12.pvf_voting_ttl                           , v13.pvf_voting_ttl);
					assert_eq!(v12.minimum_validation_upgrade_delay         , v13.minimum_validation_upgrade_delay);
					assert_eq!(v12.async_backing_params                     , v13.async_backing_params);
					assert_eq!(v12.executor_params                          , v13.executor_params);
					assert_eq!(v12.minimum_backing_votes                    , v13.minimum_backing_votes);
					assert_eq!(v12.scheduler_params.group_rotation_frequency, v13.scheduler_params.group_rotation_frequency);
					assert_eq!(v12.scheduler_params.paras_availability_period, v13.scheduler_params.paras_availability_period);
					assert_eq!(v12.scheduler_params.max_validators_per_core , v13.scheduler_params.max_validators_per_core);
					assert_eq!(v12.scheduler_params.lookahead               , v13.scheduler_params.lookahead);
					assert_eq!(v12.scheduler_params.num_cores               , v13.scheduler_params.num_cores);
					assert_eq!(v12.scheduler_params.on_demand_queue_max_size, v13.scheduler_params.on_demand_queue_max_size);
					assert_eq!(v12.scheduler_params.on_demand_target_queue_utilization, v13.scheduler_params.on_demand_target_queue_utilization);
					assert_eq!(v12.scheduler_params.on_demand_fee_variability, v13.scheduler_params.on_demand_fee_variability);
					assert_eq!(v12.scheduler_params.on_demand_base_fee      , v13.scheduler_params.on_demand_base_fee);
					// New field should default to zero.
					assert_eq!(v13.max_relay_parent_session_age, 0);
				}; // ; makes this a statement. `rustfmt::skip` cannot be put on an expression.
			}
		});
	}

	#[test]
	fn test_migrate_to_v13_no_pending() {
		let v12 = V12HostConfiguration::<polkadot_primitives::BlockNumber>::default();

		new_test_ext(Default::default()).execute_with(|| {
			v12::ActiveConfig::<Test>::set(Some(v12));
			v13::PendingConfigs::<Test>::set(None);

			// Shouldn't fail.
			migrate_to_v13::<Test>();
		});
	}
}
