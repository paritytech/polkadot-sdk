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

//! This migration changes only a configuration value and not it's structure.

use crate::configuration::{self, Config, Pallet};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::Defensive, weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{vstaging::node_features::FeatureIndex, SessionIndex};
use sp_std::vec::Vec;

use frame_support::traits::OnRuntimeUpgrade;

type V12HostConfiguration<BlockNumber> = configuration::HostConfiguration<BlockNumber>;

use super::v11::V11HostConfiguration;

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

pub struct UncheckedMigrateToV12<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for UncheckedMigrateToV12<T> {
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

// The migration doesn't change the configuration structure, it only toggles a bit in node features.
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
coretime_cores                           : pre.coretime_cores,
on_demand_retries                        : pre.on_demand_retries,
group_rotation_frequency                 : pre.group_rotation_frequency,
paras_availability_period                : pre.paras_availability_period,
scheduling_lookahead                     : pre.scheduling_lookahead,
max_validators_per_core                  : pre.max_validators_per_core,
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
on_demand_queue_max_size                 : pre.on_demand_queue_max_size,
on_demand_base_fee                       : pre.on_demand_base_fee,
on_demand_fee_variability                : pre.on_demand_fee_variability,
on_demand_target_queue_utilization       : pre.on_demand_target_queue_utilization,
on_demand_ttl                            : pre.on_demand_ttl,
minimum_backing_votes                    : pre.minimum_backing_votes,
node_features							 : pre.node_features,
approval_voting_params                   : pre.approval_voting_params
		}
	};

	let mut v12 = translate(
		v11::ActiveConfig::<T>::get()
			.defensive_proof("Could not decode old config")
			.unwrap_or_default(),
	);

	let elastic_scaling_mvp_bit_index = FeatureIndex::ElasticScalingMVP as usize;

	// Toggle bit in active config.
	if v12.node_features.len() <= elastic_scaling_mvp_bit_index {
		v12.node_features.resize(elastic_scaling_mvp_bit_index + 1, false);
	}

	v12.node_features.set(elastic_scaling_mvp_bit_index, true);

	v12::ActiveConfig::<T>::set(Some(v12));

	// Allowed to be empty.
	let pending_v11 = v11::PendingConfigs::<T>::get().unwrap_or_default();
	let mut pending_v12 = Vec::new();

	// Toggle bit in any pending configs.
	for (session, v11) in pending_v11.into_iter() {
		let mut v12 = translate(v11);
		// Toggle the elastic scaling feature which allows the runtime to accept a `CoreIndex`
		// encoded as an 8bit extension of `BackedCandidate::validator_indices` bitfield.
		if v12.node_features.len() <= elastic_scaling_mvp_bit_index {
			v12.node_features.resize(elastic_scaling_mvp_bit_index + 1, false);
		}
		v12.node_features.set(elastic_scaling_mvp_bit_index, true);

		pending_v12.push((session, v12));
	}

	let num_configs = (pending_v12.len() + 1) as u64;

	v12::PendingConfigs::<T>::set(Some(pending_v12));

	T::DbWeight::get().reads_writes(num_configs, num_configs)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

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
		let v11 = V11HostConfiguration::<primitives::BlockNumber> {
			needed_approvals: 69,
			paras_availability_period: 55,
			hrmp_recipient_deposit: 1337,
			max_pov_size: 1111,
			minimum_validation_upgrade_delay: 20,
			..Default::default()
		};

		let mut pending_configs = Vec::new();
		pending_configs.push((100, v11.clone()));
		pending_configs.push((300, v11.clone()));

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v11 version in the state.
			v11::ActiveConfig::<Test>::set(Some(v11.clone()));
			v11::PendingConfigs::<Test>::set(Some(pending_configs));

			migrate_to_v12::<Test>();

			let mut configs_to_check = v12::PendingConfigs::<Test>::get().unwrap();
			let v12 = v12::ActiveConfig::<Test>::get().unwrap();

			configs_to_check.push((0, v12));

			// This is not really needed since struct didnt change but better to be sure than sorry.
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
					assert_eq!(v11.coretime_cores                           , v12.coretime_cores);
					assert_eq!(v11.on_demand_retries                        , v12.on_demand_retries);
					assert_eq!(v11.group_rotation_frequency                 , v12.group_rotation_frequency);
					assert_eq!(v11.paras_availability_period                , v12.paras_availability_period);
					assert_eq!(v11.scheduling_lookahead                     , v12.scheduling_lookahead);
					assert_eq!(v11.max_validators_per_core                  , v12.max_validators_per_core);
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
					assert_eq!(v11.executor_params						    , v12.executor_params);
				    assert_eq!(v11.minimum_backing_votes					, v12.minimum_backing_votes);
				}; // ; makes this a statement. `rustfmt::skip` cannot be put on an expression.

				// We expect this to be true after migration.
				let elastic_scaling_mvp_bit = v12.node_features
					.get(FeatureIndex::ElasticScalingMVP as usize)
					.map(|b| *b)
					.defensive_unwrap_or(false);

				assert_eq!(elastic_scaling_mvp_bit, true);
			}
		});
	}

	// Test that migration doesn't panic in case there're no pending configurations upgrades in
	// pallet's storage.
	#[test]
	fn test_migrate_to_v12_no_pending() {
		let v11 = V11HostConfiguration::<primitives::BlockNumber>::default();

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v11 version in the state.
			v11::ActiveConfig::<Test>::set(Some(v11));
			// Ensure there're no pending configs.
			v12::PendingConfigs::<Test>::set(None);

			// Shouldn't fail.
			migrate_to_v12::<Test>();
		});
	}
}
