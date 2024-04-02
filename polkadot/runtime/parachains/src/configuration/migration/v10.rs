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

use crate::configuration::{Config, Pallet};
use frame_support::{
	pallet_prelude::*,
	traits::{Defensive, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
	AsyncBackingParams, Balance, ExecutorParams, NodeFeatures, SessionIndex,
	LEGACY_MIN_BACKING_VOTES, ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
};
use sp_runtime::Perbill;
use sp_std::vec::Vec;

use super::v9::V9HostConfiguration;
// All configuration of the runtime with respect to paras.
#[derive(Clone, Encode, PartialEq, Decode, Debug)]
pub struct V10HostConfiguration<BlockNumber> {
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
	pub on_demand_cores: u32,
	pub on_demand_retries: u32,
	pub on_demand_queue_max_size: u32,
	pub on_demand_target_queue_utilization: Perbill,
	pub on_demand_fee_variability: Perbill,
	pub on_demand_base_fee: Balance,
	pub on_demand_ttl: BlockNumber,
	pub group_rotation_frequency: BlockNumber,
	pub paras_availability_period: BlockNumber,
	pub scheduling_lookahead: u32,
	pub max_validators_per_core: Option<u32>,
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
}

impl<BlockNumber: Default + From<u32>> Default for V10HostConfiguration<BlockNumber> {
	fn default() -> Self {
		Self {
			async_backing_params: AsyncBackingParams {
				max_candidate_depth: 0,
				allowed_ancestry_len: 0,
			},
			group_rotation_frequency: 1u32.into(),
			paras_availability_period: 1u32.into(),
			no_show_slots: 1u32.into(),
			validation_upgrade_cooldown: Default::default(),
			validation_upgrade_delay: 2u32.into(),
			code_retention_period: Default::default(),
			max_code_size: Default::default(),
			max_pov_size: Default::default(),
			max_head_data_size: Default::default(),
			on_demand_cores: Default::default(),
			on_demand_retries: Default::default(),
			scheduling_lookahead: 1,
			max_validators_per_core: Default::default(),
			max_validators: None,
			dispute_period: 6,
			dispute_post_conclusion_acceptance_period: 100.into(),
			n_delay_tranches: Default::default(),
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
			on_demand_queue_max_size: ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
			on_demand_base_fee: 10_000_000u128,
			on_demand_fee_variability: Perbill::from_percent(3),
			on_demand_target_queue_utilization: Perbill::from_percent(25),
			on_demand_ttl: 5u32.into(),
			minimum_backing_votes: LEGACY_MIN_BACKING_VOTES,
			node_features: NodeFeatures::EMPTY,
		}
	}
}

mod v9 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type ActiveConfig<T: Config> =
		StorageValue<Pallet<T>, V9HostConfiguration<BlockNumberFor<T>>, OptionQuery>;

	#[frame_support::storage_alias]
	pub(crate) type PendingConfigs<T: Config> = StorageValue<
		Pallet<T>,
		Vec<(SessionIndex, V9HostConfiguration<BlockNumberFor<T>>)>,
		OptionQuery,
	>;
}

mod v10 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type ActiveConfig<T: Config> =
		StorageValue<Pallet<T>, V10HostConfiguration<BlockNumberFor<T>>, OptionQuery>;

	#[frame_support::storage_alias]
	pub(crate) type PendingConfigs<T: Config> = StorageValue<
		Pallet<T>,
		Vec<(SessionIndex, V10HostConfiguration<BlockNumberFor<T>>)>,
		OptionQuery,
	>;
}

pub struct VersionUncheckedMigrateToV10<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV10<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running pre_upgrade() for HostConfiguration MigrateToV10");
		Ok(Vec::new())
	}

	fn on_runtime_upgrade() -> Weight {
		migrate_to_v10::<T>()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		log::trace!(target: crate::configuration::LOG_TARGET, "Running post_upgrade() for HostConfiguration MigrateToV10");
		ensure!(
			Pallet::<T>::on_chain_storage_version() >= StorageVersion::new(10),
			"Storage version should be >= 10 after the migration"
		);

		Ok(())
	}
}

pub type MigrateToV10<T> = frame_support::migrations::VersionedMigration<
	9,
	10,
	VersionUncheckedMigrateToV10<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

// Unusual formatting is justified:
// - make it easier to verify that fields assign what they supposed to assign.
// - this code is transient and will be removed after all migrations are done.
// - this code is important enough to optimize for legibility sacrificing consistency.
#[rustfmt::skip]
fn translate<T: Config>(pre: V9HostConfiguration<BlockNumberFor<T>>) -> V10HostConfiguration<BlockNumberFor<T>> {
	V10HostConfiguration {
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
		on_demand_cores                          : pre.on_demand_cores,
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
		node_features                            : NodeFeatures::EMPTY
	}
}

fn migrate_to_v10<T: Config>() -> Weight {
	let v9 = v9::ActiveConfig::<T>::get()
		.defensive_proof("Could not decode old config")
		.unwrap_or_default();
	let v10 = translate::<T>(v9);
	v10::ActiveConfig::<T>::set(Some(v10));

	// Allowed to be empty.
	let pending_v9 = v9::PendingConfigs::<T>::get().unwrap_or_default();
	let mut pending_v10 = Vec::new();

	for (session, v9) in pending_v9.into_iter() {
		let v10 = translate::<T>(v9);
		pending_v10.push((session, v10));
	}
	v10::PendingConfigs::<T>::set(Some(pending_v10.clone()));

	let num_configs = (pending_v10.len() + 1) as u64;
	T::DbWeight::get().reads_writes(num_configs, num_configs)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};
	use primitives::LEGACY_MIN_BACKING_VOTES;

	#[test]
	fn v10_deserialized_from_actual_data() {
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
		// referenced        block.
		//   2.4. You'll also need the decoded values to update the test.
		// 3. Go to Polkadot.js -> Developer -> Chain state -> Raw storage
		//   3.1 Enter the encoded storage key and you get the raw config.

		// This exceeds the maximal line width length, but that's fine, since this is not code and
		// doesn't need to be read and also leaving it as one line allows to easily copy it.
		let raw_config =
	hex_literal::hex!["
	0000300000800000080000000000100000c8000005000000050000000200000002000000000000000000000000005000000010000400000000000000000000000000000000000000000000000000000000000000000000000800000000200000040000000000100000b004000000000000000000001027000080b2e60e80c3c90180969800000000000000000000000000050000001400000004000000010000000101000000000600000064000000020000001900000000000000020000000200000002000000050000000200000000"
	];

		let v10 =
			V10HostConfiguration::<primitives::BlockNumber>::decode(&mut &raw_config[..]).unwrap();

		// We check only a sample of the values here. If we missed any fields or messed up data
		// types that would skew all the fields coming after.
		assert_eq!(v10.max_code_size, 3_145_728);
		assert_eq!(v10.validation_upgrade_cooldown, 2);
		assert_eq!(v10.max_pov_size, 5_242_880);
		assert_eq!(v10.hrmp_channel_max_message_size, 1_048_576);
		assert_eq!(v10.n_delay_tranches, 25);
		assert_eq!(v10.minimum_validation_upgrade_delay, 5);
		assert_eq!(v10.group_rotation_frequency, 20);
		assert_eq!(v10.on_demand_cores, 0);
		assert_eq!(v10.on_demand_base_fee, 10_000_000);
		assert_eq!(v10.minimum_backing_votes, LEGACY_MIN_BACKING_VOTES);
		assert_eq!(v10.node_features, NodeFeatures::EMPTY);
	}

	// Test that `migrate_to_v10`` correctly applies the `translate` function to current and pending
	// configs.
	#[test]
	fn test_migrate_to_v10() {
		// Host configuration has lots of fields. However, in this migration we only add one
		// field. The most important part to check are a couple of the last fields. We also pick
		// extra fields to check arbitrarily, e.g. depending on their position (i.e. the middle) and
		// also their type.
		//
		// We specify only the picked fields and the rest should be provided by the `Default`
		// implementation. That implementation is copied over between the two types and should work
		// fine.
		let v9 = V9HostConfiguration::<primitives::BlockNumber> {
			needed_approvals: 69,
			paras_availability_period: 55,
			hrmp_recipient_deposit: 1337,
			max_pov_size: 1111,
			minimum_validation_upgrade_delay: 20,
			..Default::default()
		};

		let mut pending_configs = Vec::new();
		pending_configs.push((100, v9.clone()));
		pending_configs.push((300, v9.clone()));

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v9 version in the state.
			v9::ActiveConfig::<Test>::set(Some(v9.clone()));
			v9::PendingConfigs::<Test>::set(Some(pending_configs));

			migrate_to_v10::<Test>();

			let v10 = translate::<Test>(v9);
			let mut configs_to_check = v10::PendingConfigs::<Test>::get().unwrap();
			configs_to_check.push((0, v10::ActiveConfig::<Test>::get().unwrap()));

			for (_, config) in configs_to_check {
				assert_eq!(config, v10);
				assert_eq!(config.node_features, NodeFeatures::EMPTY);
			}
		});
	}

	// Test that migration doesn't panic in case there're no pending configurations upgrades in
	// pallet's storage.
	#[test]
	fn test_migrate_to_v10_no_pending() {
		let v9 = V9HostConfiguration::<primitives::BlockNumber>::default();

		new_test_ext(Default::default()).execute_with(|| {
			// Implant the v9 version in the state.
			v9::ActiveConfig::<Test>::set(Some(v9));
			// Ensure there're no pending configs.
			v9::PendingConfigs::<Test>::set(None);

			// Shouldn't fail.
			migrate_to_v10::<Test>();
		});
	}
}
