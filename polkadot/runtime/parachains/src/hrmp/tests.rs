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

// NOTE: System chains, identified by ParaId < 2000, are treated as special in HRMP channel
// initialization. Namely, they do not require a deposit if even one ParaId is a system para. If
// both paras are system chains, then they are also configured to the system's max configuration.

use super::*;
use crate::{
	mock::{
		deregister_parachain, new_test_ext, register_parachain, register_parachain_with_balance,
		Dmp, Hrmp, MaxRequestsPerWindow, MockGenesisConfig, Paras, ParasShared, RequestWindow,
		RuntimeEvent as MockEvent, RuntimeOrigin, System, Test, TestUsesOnlyStoredVersionWrapper,
	},
	shared,
};
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives::{BlockNumber, InboundDownwardMessage};
use sp_runtime::traits::BadOrigin;
use std::collections::BTreeMap;

pub(crate) fn run_to_block(to: BlockNumber, new_session: Option<Vec<BlockNumber>>) {
	let config = configuration::ActiveConfig::<Test>::get();
	while System::block_number() < to {
		let b = System::block_number();

		// NOTE: this is in reverse initialization order.
		Hrmp::initializer_finalize();
		Paras::initializer_finalize(b);
		ParasShared::initializer_finalize();

		if new_session.as_ref().map_or(false, |v| v.contains(&(b + 1))) {
			let notification = crate::initializer::SessionChangeNotification {
				prev_config: config.clone(),
				new_config: config.clone(),
				session_index: shared::CurrentSessionIndex::<Test>::get() + 1,
				..Default::default()
			};

			// NOTE: this is in initialization order.
			ParasShared::initializer_on_new_session(
				notification.session_index,
				notification.random_seed,
				&notification.new_config,
				notification.validators.clone(),
			);
			let outgoing_paras = Paras::initializer_on_new_session(&notification);
			Hrmp::initializer_on_new_session(&notification, &outgoing_paras);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		// NOTE: this is in initialization order.
		ParasShared::initializer_initialize(b + 1);
		Paras::initializer_initialize(b + 1);
		Hrmp::initializer_initialize(b + 1);
	}
}

#[derive(Debug)]
pub(super) struct GenesisConfigBuilder {
	hrmp_channel_max_capacity: u32,
	hrmp_channel_max_message_size: u32,
	hrmp_max_paras_outbound_channels: u32,
	hrmp_max_paras_inbound_channels: u32,
	hrmp_max_message_num_per_candidate: u32,
	hrmp_channel_max_total_size: u32,
	hrmp_sender_deposit: Balance,
	hrmp_recipient_deposit: Balance,
}

impl Default for GenesisConfigBuilder {
	fn default() -> Self {
		Self {
			hrmp_channel_max_capacity: 2,
			hrmp_channel_max_message_size: 8,
			hrmp_max_paras_outbound_channels: 2,
			hrmp_max_paras_inbound_channels: 2,
			hrmp_max_message_num_per_candidate: 2,
			hrmp_channel_max_total_size: 16,
			hrmp_sender_deposit: 100,
			hrmp_recipient_deposit: 100,
		}
	}
}

impl GenesisConfigBuilder {
	pub(super) fn build(self) -> crate::mock::MockGenesisConfig {
		let mut genesis = default_genesis_config();
		let config = &mut genesis.configuration.config;
		config.hrmp_channel_max_capacity = self.hrmp_channel_max_capacity;
		config.hrmp_channel_max_message_size = self.hrmp_channel_max_message_size;
		config.hrmp_max_parachain_outbound_channels = self.hrmp_max_paras_outbound_channels;
		config.hrmp_max_parachain_inbound_channels = self.hrmp_max_paras_inbound_channels;
		config.hrmp_max_message_num_per_candidate = self.hrmp_max_message_num_per_candidate;
		config.hrmp_channel_max_total_size = self.hrmp_channel_max_total_size;
		config.hrmp_sender_deposit = self.hrmp_sender_deposit;
		config.hrmp_recipient_deposit = self.hrmp_recipient_deposit;
		genesis
	}
}

fn default_genesis_config() -> MockGenesisConfig {
	MockGenesisConfig {
		configuration: crate::configuration::GenesisConfig {
			config: crate::configuration::HostConfiguration {
				max_downward_message_size: 1024,
				..Default::default()
			},
		},
		..Default::default()
	}
}

fn channel_exists(sender: ParaId, recipient: ParaId) -> bool {
	HrmpChannels::<Test>::get(&HrmpChannelId { sender, recipient }).is_some()
}

#[test]
fn empty_state_consistent_state() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn open_channel_works() {
	let para_a = 2001.into();
	let para_a_origin: crate::Origin = 2001.into();
	let para_b = 2003.into();
	let para_b_origin: crate::Origin = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and alive parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::hrmp_init_open_channel(para_a_origin.into(), para_b, 2, 8).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::OpenChannelRequested {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		Hrmp::hrmp_accept_open_channel(para_b_origin.into(), para_a).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::OpenChannelAccepted { sender: para_a, recipient: para_b })));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
	});
}

#[test]
fn force_open_channel_works() {
	let para_a = 1.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		let para_a_free_balance =
			<Test as Config>::Currency::free_balance(&para_a.into_account_truncating());
		let para_b_free_balance =
			<Test as Config>::Currency::free_balance(&para_b.into_account_truncating());

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_a, para_b, 2, 8).unwrap();
		Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_b, para_a, 2, 8).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpChannelForceOpened {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpChannelForceOpened {
				sender: para_b,
				recipient: para_a,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		assert!(!channel_exists(para_b, para_a));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
		assert!(channel_exists(para_b, para_a));
		// Because para_a is a system chain, their free balances should not have changed.
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()),
			para_a_free_balance
		);
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()),
			para_b_free_balance
		);
	});
}

#[test]
fn force_open_channel_without_free_balance_works() {
	let para_a = 1.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains, but they should not have any
		// balance in their sovereign accounts. Even without any balance, the channel opening should
		// still be successful.
		register_parachain_with_balance(para_a, 0);
		register_parachain_with_balance(para_b, 0);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_a, para_b, 2, 8).unwrap();
		Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_b, para_a, 2, 8).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpChannelForceOpened {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpChannelForceOpened {
				sender: para_b,
				recipient: para_a,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		assert!(!channel_exists(para_b, para_a));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
		assert!(channel_exists(para_b, para_a));
	});
}

#[test]
fn force_open_channel_works_with_existing_request() {
	let para_a = 2001.into();
	let para_a_origin: crate::Origin = 2001.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		// Request a channel from `a` to `b`.
		run_to_block(3, Some(vec![2, 3]));
		Hrmp::hrmp_init_open_channel(para_a_origin.into(), para_b, 2, 8).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::OpenChannelRequested {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		run_to_block(5, Some(vec![4, 5]));
		// the request exists, but no channel.
		assert!(HrmpOpenChannelRequests::<Test>::get(&HrmpChannelId {
			sender: para_a,
			recipient: para_b
		})
		.is_some());
		assert!(!channel_exists(para_a, para_b));
		// now force open it.
		Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_a, para_b, 2, 8).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpChannelForceOpened {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
	});
}

#[test]
fn force_open_channel_with_invalid_sender_fails() {
	let para_a = 2001.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Only B is registered.
		register_parachain(para_b);
		run_to_block(5, Some(vec![4, 5]));

		assert_noop!(
			Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), para_a, para_b, 2, 8),
			Error::<Test>::OpenHrmpChannelInvalidSender
		);
	});
}

#[test]
fn open_system_channel_works() {
	let para_a = 1.into();
	let para_b = 3.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::establish_system_channel(RuntimeOrigin::signed(1), para_a, para_b).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpSystemChannelOpened {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8
			})));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
	});
}

#[test]
fn open_system_channel_does_not_work_for_non_system_chains() {
	let para_a = 2001.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		assert_noop!(
			Hrmp::establish_system_channel(RuntimeOrigin::signed(1), para_a, para_b),
			Error::<Test>::ChannelCreationNotAuthorized
		);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn open_system_channel_does_not_work_with_one_non_system_chain() {
	let para_a = 1.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		assert_noop!(
			Hrmp::establish_system_channel(RuntimeOrigin::signed(1), para_a, para_b),
			Error::<Test>::ChannelCreationNotAuthorized
		);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn open_system_channel_origins() {
	let para_a = 1.into();
	let para_b = 3.into();
	let para_c = 4.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);
		register_parachain(para_c);
		run_to_block(5, Some(vec![4, 5]));

		// A non-system para may not.
		let non_system: crate::Origin = 2001.into();
		assert_noop!(
			Hrmp::establish_system_channel(non_system.into(), para_a, para_b),
			Error::<Test>::ChannelCreationNotAuthorized
		);
		// The `ChannelManager` may, and so may any system para, even one not in the channel.
		assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), para_a, para_b));
		let outsider: crate::Origin = 1.into();
		assert_ok!(Hrmp::establish_system_channel(outsider.into(), para_b, para_c));
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn poke_deposits_origins() {
	let para_a = 2001.into();
	let para_b = 2002.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 100);
		insert_channel_without_deposits(&HrmpChannelId { sender: para_a, recipient: para_b });

		// A para outside of the channel may not.
		let other: crate::Origin = 2003.into();
		assert_noop!(
			Hrmp::poke_channel_deposits(other.into(), para_a, para_b),
			Error::<Test>::PokeHrmpChannelUnauthorized
		);
		// Neither may a system para outside of the channel.
		let system: crate::Origin = 1.into();
		assert_noop!(
			Hrmp::poke_channel_deposits(system.into(), para_a, para_b),
			Error::<Test>::PokeHrmpChannelUnauthorized
		);
		// Either side of the channel and the `ChannelManager` may.
		let sender: crate::Origin = 2001.into();
		assert_ok!(Hrmp::poke_channel_deposits(sender.into(), para_a, para_b));
		let recipient: crate::Origin = 2002.into();
		assert_ok!(Hrmp::poke_channel_deposits(recipient.into(), para_a, para_b));
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::root(), para_a, para_b));
	});
}

#[test]
fn poke_deposits_works() {
	let para_a = 1.into();
	let para_b = 2001.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain_with_balance(para_a, 200);
		register_parachain_with_balance(para_b, 200);

		let config = configuration::ActiveConfig::<Test>::get();
		let channel_id = HrmpChannelId { sender: para_a, recipient: para_b };

		// Our normal establishment won't actually reserve deposits, so just insert them directly.
		HrmpChannels::<Test>::insert(
			&channel_id,
			HrmpChannel {
				sender_deposit: config.hrmp_sender_deposit,
				recipient_deposit: config.hrmp_recipient_deposit,
				max_capacity: config.hrmp_channel_max_capacity,
				max_total_size: config.hrmp_channel_max_total_size,
				max_message_size: config.hrmp_channel_max_message_size,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);
		// reserve funds
		assert_ok!(<Test as Config>::Currency::reserve(
			&para_a.into_account_truncating(),
			config.hrmp_sender_deposit
		));
		assert_ok!(<Test as Config>::Currency::reserve(
			&para_b.into_account_truncating(),
			config.hrmp_recipient_deposit
		));

		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), para_a, para_b));

		assert_eq!(
			<Test as Config>::Currency::reserved_balance(&para_a.into_account_truncating()),
			0
		);
		assert_eq!(
			<Test as Config>::Currency::reserved_balance(&para_b.into_account_truncating()),
			0
		);
	});
}

fn insert_channel_without_deposits(channel_id: &HrmpChannelId) {
	let config = configuration::ActiveConfig::<Test>::get();
	HrmpChannels::<Test>::insert(
		channel_id,
		HrmpChannel {
			sender_deposit: 0,
			recipient_deposit: 0,
			max_capacity: config.hrmp_channel_max_capacity,
			max_total_size: config.hrmp_channel_max_total_size,
			max_message_size: config.hrmp_channel_max_message_size,
			msg_count: 0,
			total_size: 0,
			mqc_head: None,
		},
	);
}

#[test]
fn poke_deposits_increase_works() {
	let para_a = 2001.into();
	let para_b = 2002.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 110);
		let channel_id = HrmpChannelId { sender: para_a, recipient: para_b };
		insert_channel_without_deposits(&channel_id);

		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), para_a, para_b));

		let channel = HrmpChannels::<Test>::get(&channel_id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (20, 15));
		assert_eq!(
			<Test as Config>::Currency::reserved_balance(&para_a.into_account_truncating()),
			20
		);
		assert_eq!(
			<Test as Config>::Currency::reserved_balance(&para_b.into_account_truncating()),
			15
		);
	});
}

#[test]
fn poke_deposits_increase_without_balance_fails() {
	let para_a = 2001.into();
	let para_b = 2002.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 0);
		insert_channel_without_deposits(&HrmpChannelId { sender: para_a, recipient: para_b });

		// The sender's deposit is taken first and must be rolled back too.
		assert_noop!(
			Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), para_a, para_b),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn close_channel_works() {
	let para_a = 2005.into();
	let para_b = 2002.into();
	let para_b_origin: crate::Origin = 2002.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();

		run_to_block(6, Some(vec![6]));
		assert!(channel_exists(para_a, para_b));

		// Close the channel. The effect is not immediate, but rather deferred to the next
		// session change.
		let channel_id = HrmpChannelId { sender: para_a, recipient: para_b };
		Hrmp::hrmp_close_channel(para_b_origin.into(), channel_id.clone()).unwrap();
		assert!(channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();

		// After the session change the channel should be closed.
		run_to_block(8, Some(vec![8]));
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::ChannelClosed {
				by_parachain: para_b,
				channel_id: channel_id.clone()
			})));
	});
}

#[test]
fn send_recv_messages() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_channel_max_message_size = 20;
	genesis.hrmp_channel_max_total_size = 20;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 20).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();

		// On Block 6:
		// A sends a message to B
		run_to_block(6, Some(vec![6]));
		assert!(channel_exists(para_a, para_b));
		let msgs: HorizontalMessages =
			vec![OutboundHrmpMessage { recipient: para_b, data: b"this is an emergency".to_vec() }]
				.try_into()
				.unwrap();
		let config = configuration::ActiveConfig::<Test>::get();
		assert!(Hrmp::check_outbound_hrmp(&config, para_a, &msgs).is_ok());
		let _ = Hrmp::queue_outbound_hrmp(para_a, msgs);
		Hrmp::assert_storage_consistency_exhaustive();

		// On Block 7:
		// B receives the message sent by A. B sets the watermark to 6.
		run_to_block(7, None);
		assert!(Hrmp::check_hrmp_watermark(para_b, 7, 6).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 6);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn hrmp_mqc_head_fixture() {
	let para_a = 2000.into();
	let para_b = 2024.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_channel_max_message_size = 20;
	genesis.hrmp_channel_max_total_size = 20;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(2, Some(vec![1, 2]));
		Hrmp::init_open_channel(para_a, para_b, 2, 20).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();

		run_to_block(3, Some(vec![3]));
		let _ = Hrmp::queue_outbound_hrmp(
			para_a,
			vec![OutboundHrmpMessage { recipient: para_b, data: vec![1, 2, 3] }]
				.try_into()
				.unwrap(),
		);

		run_to_block(4, None);
		let _ = Hrmp::queue_outbound_hrmp(
			para_a,
			vec![OutboundHrmpMessage { recipient: para_b, data: vec![4, 5, 6] }]
				.try_into()
				.unwrap(),
		);

		assert_eq!(
			Hrmp::hrmp_mqc_heads(para_b),
			vec![(
				para_a,
				hex_literal::hex![
					"a964fd3b4f3d3ce92a0e25e576b87590d92bb5cb7031909c7f29050e1f04a375"
				]
				.into()
			),],
		);
	});
}

#[test]
fn accept_incoming_request_and_offboard() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		deregister_parachain(para_a);

		// On Block 7: 2x session change. The channel should not be created.
		run_to_block(7, Some(vec![6, 7]));
		assert!(!Paras::is_valid_para(para_a));
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn check_sent_messages() {
	let para_a = 2032.into();
	let para_b = 2064.into();
	let para_c = 2097.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);
		register_parachain(para_c);

		run_to_block(5, Some(vec![4, 5]));

		// Open two channels to the same receiver, b:
		// a -> b, c -> b
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		Hrmp::init_open_channel(para_c, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_c).unwrap();

		// On Block 6: session change.
		run_to_block(6, Some(vec![6]));
		assert!(Paras::is_valid_para(para_a));

		let msgs: HorizontalMessages =
			vec![OutboundHrmpMessage { recipient: para_b, data: b"knock".to_vec() }]
				.try_into()
				.unwrap();
		let config = configuration::ActiveConfig::<Test>::get();
		assert!(Hrmp::check_outbound_hrmp(&config, para_a, &msgs).is_ok());
		let _ = Hrmp::queue_outbound_hrmp(para_a, msgs.clone());

		// Verify that the sent messages are there and that also the empty channels are present.
		let mqc_heads = Hrmp::hrmp_mqc_heads(para_b);
		let contents = Hrmp::inbound_hrmp_channels_contents(para_b);
		assert_eq!(
			contents,
			vec![
				(para_a, vec![InboundHrmpMessage { sent_at: 6, data: b"knock".to_vec() }]),
				(para_c, vec![])
			]
			.into_iter()
			.collect::<BTreeMap::<_, _>>(),
		);
		assert_eq!(
			mqc_heads,
			vec![
				(
					para_a,
					hex_literal::hex!(
						"3bba6404e59c91f51deb2ae78f1273ebe75896850713e13f8c0eba4b0996c483"
					)
					.into()
				),
				(para_c, Default::default())
			],
		);

		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn verify_externally_accessible() {
	use polkadot_primitives::{well_known_keys, AbridgedHrmpChannel};

	let para_a = 2020.into();
	let para_b = 2021.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Register two parachains, wait until a session change, then initiate channel open
		// request and accept that, and finally wait until the next session.
		register_parachain(para_a);
		register_parachain(para_b);
		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		run_to_block(8, Some(vec![8]));

		// Here we have a channel a->b opened.
		//
		// Try to obtain this channel from the storage and
		// decode it into the abridged version.
		assert!(channel_exists(para_a, para_b));
		let raw_hrmp_channel =
			sp_io::storage::get(&well_known_keys::hrmp_channels(HrmpChannelId {
				sender: para_a,
				recipient: para_b,
			}))
			.expect("the channel exists and we must be able to get it through well known keys");
		let abridged_hrmp_channel = AbridgedHrmpChannel::decode(&mut &raw_hrmp_channel[..])
			.expect("HrmpChannel should be decodable as AbridgedHrmpChannel");

		assert_eq!(
			abridged_hrmp_channel,
			AbridgedHrmpChannel {
				max_capacity: 2,
				max_total_size: 16,
				max_message_size: 8,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);

		let raw_ingress_index =
			sp_io::storage::get(&well_known_keys::hrmp_ingress_channel_index(para_b))
				.expect("the ingress index must be present for para_b");
		let ingress_index = <Vec<ParaId>>::decode(&mut &raw_ingress_index[..])
			.expect("ingress index should be decodable as a list of para ids");
		assert_eq!(ingress_index, vec![para_a]);

		// Now, verify that we can access and decode the egress index.
		let raw_egress_index =
			sp_io::storage::get(&well_known_keys::hrmp_egress_channel_index(para_a))
				.expect("the egress index must be present for para_a");
		let egress_index = <Vec<ParaId>>::decode(&mut &raw_egress_index[..])
			.expect("egress index should be decodable as a list of para ids");
		assert_eq!(egress_index, vec![para_b]);
	});
}

#[test]
fn charging_deposits() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain_with_balance(para_a, 0);
		register_parachain(para_b);
		run_to_block(5, Some(vec![4, 5]));

		assert_noop!(
			Hrmp::init_open_channel(para_a, para_b, 2, 8),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);
	});

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain_with_balance(para_b, 0);
		run_to_block(5, Some(vec![4, 5]));

		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();

		assert_noop!(
			Hrmp::accept_open_channel(para_b, para_a),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn refund_deposit_on_normal_closure() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		// Register two parachains funded with different amounts of funds and arrange a channel.
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 110);
		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		assert_eq!(<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()), 80);
		assert_eq!(<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()), 95);
		run_to_block(8, Some(vec![8]));

		// Now, we close the channel and wait until the next session.
		Hrmp::close_channel(para_b, HrmpChannelId { sender: para_a, recipient: para_b }).unwrap();
		run_to_block(10, Some(vec![10]));
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()),
			100
		);
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()),
			110
		);
	});
}

#[test]
fn refund_deposit_on_offboarding() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		// Register two parachains and open a channel between them.
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 110);
		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		assert_eq!(<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()), 80);
		assert_eq!(<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()), 95);
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));

		// Then deregister one parachain.
		deregister_parachain(para_a);
		run_to_block(10, Some(vec![9, 10]));

		// The channel should be removed.
		assert!(!Paras::is_valid_para(para_a));
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();

		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()),
			100
		);
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()),
			110
		);
	});
}

#[test]
fn no_dangling_open_requests() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		// Register two parachains and open a channel between them.
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 110);
		run_to_block(5, Some(vec![4, 5]));

		// Start opening a channel a->b
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		assert_eq!(<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()), 80);

		// Then deregister one parachain, but don't wait two sessions until it takes effect.
		// Instead, `para_b` will confirm the request, which will take place the same time
		// the offboarding should happen.
		deregister_parachain(para_a);
		run_to_block(9, Some(vec![9]));
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		assert_eq!(<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()), 95);
		assert!(!channel_exists(para_a, para_b));
		run_to_block(10, Some(vec![10]));

		// The outcome we expect is `para_b` should receive the refund.
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_b.into_account_truncating()),
			110
		);
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn cancel_pending_open_channel_request() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	new_test_ext(genesis.build()).execute_with(|| {
		// Register two parachains and open a channel between them.
		register_parachain_with_balance(para_a, 100);
		register_parachain_with_balance(para_b, 110);
		run_to_block(5, Some(vec![4, 5]));

		// Start opening a channel a->b
		Hrmp::init_open_channel(para_a, para_b, 2, 8).unwrap();
		assert_eq!(<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()), 80);

		// Cancel opening the channel
		Hrmp::cancel_open_request(para_a, HrmpChannelId { sender: para_a, recipient: para_b })
			.unwrap();
		assert_eq!(
			<Test as Config>::Currency::free_balance(&para_a.into_account_truncating()),
			100
		);

		run_to_block(10, Some(vec![10]));
		assert!(!channel_exists(para_a, para_b));
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn watermark_can_remain_the_same() {
	let para_a = 2032.into();
	let para_b = 2064.into();
	let para_c = 3000.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_channel_max_message_size = 20;
	genesis.hrmp_channel_max_total_size = 20;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);
		register_parachain(para_c);

		run_to_block(3, Some(vec![2, 3]));
		Hrmp::init_open_channel(para_a, para_b, 2, 20).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();
		Hrmp::init_open_channel(para_c, para_b, 2, 20).unwrap();
		Hrmp::accept_open_channel(para_b, para_c).unwrap();

		// Update watermark
		assert!(Hrmp::check_hrmp_watermark(para_b, 3, 3).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 3);

		// On Block 6:
		// A sends 1 message to B
		run_to_block(6, Some(vec![6]));
		assert!(channel_exists(para_a, para_b));
		let msgs: HorizontalMessages =
			vec![OutboundHrmpMessage { recipient: para_b, data: b"HRMP message from A".to_vec() }]
				.try_into()
				.unwrap();
		let config = configuration::ActiveConfig::<Test>::get();
		assert!(Hrmp::check_outbound_hrmp(&config, para_a, &msgs).is_ok());
		let _ = Hrmp::queue_outbound_hrmp(para_a, msgs);
		Hrmp::assert_storage_consistency_exhaustive();
		// C sends 1 message to B
		assert!(channel_exists(para_c, para_b));
		let msgs: HorizontalMessages =
			vec![OutboundHrmpMessage { recipient: para_b, data: b"HRMP message from C".to_vec() }]
				.try_into()
				.unwrap();
		let config = configuration::ActiveConfig::<Test>::get();
		assert!(Hrmp::check_outbound_hrmp(&config, para_c, &msgs).is_ok());
		let _ = Hrmp::queue_outbound_hrmp(para_c, msgs);
		Hrmp::assert_storage_consistency_exhaustive();

		// Check that a smaller HRMP watermark is not accepted
		assert!(matches!(
			Hrmp::check_hrmp_watermark(para_b, 6, 2),
			Err(HrmpWatermarkAcceptanceErr::AdvancementRule {
				new_watermark: 2,
				last_watermark: 3
			})
		));

		// Check that an HRMP watermark representing a relay chain block that doesn't contain
		// any message is not accepted
		assert!(matches!(
			Hrmp::check_hrmp_watermark(para_b, 6, 5),
			Err(HrmpWatermarkAcceptanceErr::LandsOnBlockWithNoMessages { new_watermark: 5 })
		));

		// On block 7:
		// B receives the messages, but can process only the one sent by A.
		// B keeps the old watermark.
		run_to_block(7, None);
		assert!(Hrmp::check_hrmp_watermark(para_b, 7, 3).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 5);
		Hrmp::assert_storage_consistency_exhaustive();

		// On block 8:
		// B can also process the message sent by C. B sets the watermark to 6.
		run_to_block(8, None);
		assert!(Hrmp::check_hrmp_watermark(para_b, 7, 6).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 6);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn watermark_maxed_out_at_relay_parent() {
	let para_a = 2032.into();
	let para_b = 2064.into();

	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_channel_max_message_size = 20;
	genesis.hrmp_channel_max_total_size = 20;
	new_test_ext(genesis.build()).execute_with(|| {
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::init_open_channel(para_a, para_b, 2, 20).unwrap();
		Hrmp::accept_open_channel(para_b, para_a).unwrap();

		// On Block 6:
		// A sends a message to B
		run_to_block(6, Some(vec![6]));
		assert!(channel_exists(para_a, para_b));
		let msgs: HorizontalMessages =
			vec![OutboundHrmpMessage { recipient: para_b, data: b"this is an emergency".to_vec() }]
				.try_into()
				.unwrap();
		let config = configuration::ActiveConfig::<Test>::get();
		assert!(Hrmp::check_outbound_hrmp(&config, para_a, &msgs).is_ok());
		let _ = Hrmp::queue_outbound_hrmp(para_a, msgs);
		Hrmp::assert_storage_consistency_exhaustive();

		// Check that an HRMP watermark greater than the relay parent is not accepted
		assert!(matches!(
			Hrmp::check_hrmp_watermark(para_b, 6, 7),
			Err(HrmpWatermarkAcceptanceErr::AheadRelayParent {
				new_watermark: 7,
				relay_chain_parent_number: 6,
			})
		));

		// On block 8:
		// B receives the message sent by A. B sets the watermark to 7.
		run_to_block(8, None);
		assert!(Hrmp::check_hrmp_watermark(para_b, 7, 7).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 7);
		Hrmp::assert_storage_consistency_exhaustive();

		// On block 9:
		// B includes a candidate with the same relay parent as before.
		run_to_block(9, None);
		assert!(Hrmp::check_hrmp_watermark(para_b, 7, 7).is_ok());
		let _ = Hrmp::prune_hrmp(para_b, 7);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn establish_channel_with_system_works() {
	let para_a = 2000.into();
	let para_a_origin: crate::Origin = 2000.into();
	let para_b = 3.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		Hrmp::establish_channel_with_system(para_a_origin.into(), para_b).unwrap();
		Hrmp::assert_storage_consistency_exhaustive();
		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpSystemChannelOpened {
				sender: para_a,
				recipient: para_b,
				proposed_max_capacity: 1,
				proposed_max_message_size: 4
			})));

		assert!(System::events().iter().any(|record| record.event ==
			MockEvent::Hrmp(Event::HrmpSystemChannelOpened {
				sender: para_b,
				recipient: para_a,
				proposed_max_capacity: 1,
				proposed_max_message_size: 4
			})));

		// Advance to a block 6, but without session change. That means that the channel has
		// not been created yet.
		run_to_block(6, None);
		assert!(!channel_exists(para_a, para_b));
		assert!(!channel_exists(para_b, para_a));
		Hrmp::assert_storage_consistency_exhaustive();

		// Now let the session change happen and thus open the channel.
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(para_a, para_b));
		assert!(channel_exists(para_b, para_a));
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn establish_channel_with_system_with_invalid_args() {
	let para_a = 2001.into();
	let para_a_origin: crate::Origin = 2000.into();
	let para_b = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and live parachains.
		register_parachain(para_a);
		register_parachain(para_b);

		run_to_block(5, Some(vec![4, 5]));
		assert_noop!(
			Hrmp::establish_channel_with_system(RuntimeOrigin::signed(1), para_b),
			BadOrigin
		);
		assert_noop!(
			Hrmp::establish_channel_with_system(para_a_origin.into(), para_b),
			Error::<Test>::ChannelCreationNotAuthorized
		);
		Hrmp::assert_storage_consistency_exhaustive();
	});
}

#[test]
fn hrmp_notifications_works() {
	use xcm::{
		opaque::{
			latest::{prelude::*, Xcm},
			VersionedXcm,
		},
		IntoVersion,
	};

	let para_a = 2001.into();
	let para_a_origin: crate::Origin = 2001.into();
	let para_b = 2003.into();
	let para_b_origin: crate::Origin = 2003.into();

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// We need both A & B to be registered and alive parachains.
		register_parachain(para_a);
		register_parachain(para_b);
		run_to_block(5, Some(vec![4, 5]));

		// set XCM versions for wrapper

		// for para_a -> `None`, means we will use latest.
		TestUsesOnlyStoredVersionWrapper::set_version(
			Location::new(0, [Junction::Parachain(para_a.into())]),
			None,
		);
		// for para_b -> `Some(latest - 1)`, means we will use latest-1 XCM version.
		let previous_version = XCM_VERSION - 1;
		TestUsesOnlyStoredVersionWrapper::set_version(
			Location::new(0, [Junction::Parachain(para_b.into())]),
			Some(previous_version),
		);

		let assert_notification_for = |sent_at, para_id, expected| {
			assert_eq!(
				Dmp::dmq_contents_do_not_call_in_consensus(para_id),
				vec![InboundDownwardMessage { sent_at, msg: expected }]
			);
		};

		// init open channel requests
		assert_ok!(Hrmp::hrmp_init_open_channel(para_a_origin.clone().into(), para_b, 2, 8));
		assert_ok!(Hrmp::hrmp_init_open_channel(para_b_origin.clone().into(), para_a, 2, 8));
		Hrmp::assert_storage_consistency_exhaustive();

		// check dmp notications
		assert_notification_for(
			5,
			para_b,
			VersionedXcm::from(Xcm(vec![HrmpNewChannelOpenRequest {
				sender: u32::from(para_a),
				max_capacity: 2,
				max_message_size: 8,
			}]))
			.into_version(previous_version)
			.expect("compatible")
			.encode(),
		);
		assert_notification_for(
			5,
			para_a,
			VersionedXcm::from(Xcm(vec![HrmpNewChannelOpenRequest {
				sender: u32::from(para_b),
				max_capacity: 2,
				max_message_size: 8,
			}]))
			.encode(),
		);
		let _ = Dmp::prune_dmq(para_a, 1000);
		let _ = Dmp::prune_dmq(para_b, 1000);

		// accept open channel requests
		assert_ok!(Hrmp::hrmp_accept_open_channel(para_a_origin.clone().into(), para_b));
		assert_ok!(Hrmp::hrmp_accept_open_channel(para_b_origin.clone().into(), para_a));
		Hrmp::assert_storage_consistency_exhaustive();

		// check dmp notications
		assert_notification_for(
			5,
			para_b,
			VersionedXcm::from(Xcm(vec![HrmpChannelAccepted { recipient: u32::from(para_a) }]))
				.into_version(previous_version)
				.expect("compatible")
				.encode(),
		);
		assert_notification_for(
			5,
			para_a,
			VersionedXcm::from(Xcm(vec![HrmpChannelAccepted { recipient: u32::from(para_b) }]))
				.encode(),
		);
		let _ = Dmp::prune_dmq(para_a, 1000);
		let _ = Dmp::prune_dmq(para_b, 1000);

		// On Block 6: session change - creates channel.
		run_to_block(6, Some(vec![6]));
		assert!(channel_exists(para_a, para_b));

		// close channel requests
		assert_ok!(Hrmp::hrmp_close_channel(
			para_a_origin.into(),
			HrmpChannelId { sender: para_a, recipient: para_b }
		));
		assert_ok!(Hrmp::hrmp_close_channel(
			para_b_origin.into(),
			HrmpChannelId { sender: para_b, recipient: para_a }
		));
		Hrmp::assert_storage_consistency_exhaustive();

		// check dmp notications
		assert_notification_for(
			6,
			para_b,
			VersionedXcm::from(Xcm(vec![HrmpChannelClosing {
				initiator: u32::from(para_a),
				sender: u32::from(para_a),
				recipient: u32::from(para_b),
			}]))
			.into_version(previous_version)
			.expect("compatible")
			.encode(),
		);
		assert_notification_for(
			6,
			para_a,
			VersionedXcm::from(Xcm(vec![HrmpChannelClosing {
				initiator: u32::from(para_b),
				sender: u32::from(para_b),
				recipient: u32::from(para_a),
			}]))
			.encode(),
		);
	});
}

/// The origin of the parachain `para`.
fn para_origin(para: u32) -> RuntimeOrigin {
	crate::Origin::from(para).into()
}

/// The channel from `sender` to `recipient`.
fn channel(sender: u32, recipient: u32) -> HrmpChannelId {
	HrmpChannelId { sender: sender.into(), recipient: recipient.into() }
}

/// The window of `para`: the block it started at and the requests counted in it.
fn window(para: u32) -> Option<(BlockNumber, u32)> {
	RequestWindows::<Test>::get(ParaId::from(para))
}

/// How many requests are counted in the window of `para`.
fn counted(para: u32) -> u32 {
	window(para).map_or(0, |(_, used)| used)
}

/// A genesis with room for many channels per para, and channels free of deposits.
fn free_channels_genesis() -> MockGenesisConfig {
	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_max_paras_outbound_channels = 10;
	genesis.hrmp_max_paras_inbound_channels = 10;
	genesis.hrmp_sender_deposit = 0;
	genesis.hrmp_recipient_deposit = 0;
	genesis.build()
}

/// Every call a non-system para makes with its own origin is counted for it, and for it only.
#[test]
fn every_request_of_a_para_is_counted() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		for para in [2001, 2002, 1000] {
			register_parachain(para.into());
		}
		run_to_block(5, Some(vec![4, 5]));

		// Opening, cancelling and accepting a channel.
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(2001), 2002.into(), 2, 8));
		assert_eq!((counted(2001), counted(2002)), (1, 0));
		assert_ok!(Hrmp::hrmp_cancel_open_request(para_origin(2002), channel(2001, 2002), 1));
		assert_eq!((counted(2001), counted(2002)), (1, 1));
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(2001), 2002.into(), 2, 8));
		assert_ok!(Hrmp::hrmp_accept_open_channel(para_origin(2002), 2001.into()));
		assert_eq!((counted(2001), counted(2002)), (2, 2));

		// Poking and closing it once it is open.
		run_to_block(8, Some(vec![8]));
		assert_ok!(Hrmp::poke_channel_deposits(para_origin(2001), 2001.into(), 2002.into()));
		assert_eq!((counted(2001), counted(2002)), (3, 2));
		assert_ok!(Hrmp::hrmp_close_channel(para_origin(2002), channel(2001, 2002)));
		assert_eq!((counted(2001), counted(2002)), (3, 3));

		// Both channels with a system para are one request.
		assert_ok!(Hrmp::establish_channel_with_system(para_origin(2001), 1000.into()));
		assert_eq!((counted(2001), counted(1000)), (4, 0));

		// All in the window that started at the first request.
		assert_eq!(window(2001), Some((5, 4)));
	});
}

/// A para that made all the requests of its window is refused any other, with no trace left.
/// Nobody else is held back.
#[test]
fn too_many_requests_are_refused() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		MaxRequestsPerWindow::set(4);
		for para in [2001, 2002, 2003, 1000] {
			register_parachain(para.into());
		}
		run_to_block(5, Some(vec![4, 5]));
		insert_channel_without_deposits(&channel(2001, 2002));
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(2003), 2001.into(), 2, 8));

		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(2001), 2003.into(), 2, 8));
		for _ in 0..3 {
			assert_ok!(Hrmp::poke_channel_deposits(para_origin(2001), 2001.into(), 2002.into()));
		}
		assert_eq!(counted(2001), 4);

		assert_noop!(
			Hrmp::hrmp_init_open_channel(para_origin(2001), 1000.into(), 2, 8),
			Error::<Test>::TooManyRequests
		);
		assert_noop!(
			Hrmp::hrmp_accept_open_channel(para_origin(2001), 2003.into()),
			Error::<Test>::TooManyRequests
		);
		assert_noop!(
			Hrmp::hrmp_cancel_open_request(para_origin(2001), channel(2001, 2003), 2),
			Error::<Test>::TooManyRequests
		);
		assert_noop!(
			Hrmp::hrmp_close_channel(para_origin(2001), channel(2001, 2002)),
			Error::<Test>::TooManyRequests
		);
		assert_noop!(
			Hrmp::poke_channel_deposits(para_origin(2001), 2001.into(), 2002.into()),
			Error::<Test>::TooManyRequests
		);
		assert_noop!(
			Hrmp::establish_channel_with_system(para_origin(2001), 1000.into()),
			Error::<Test>::TooManyRequests
		);

		// The other side of each of its channels and requests is not held back.
		assert_ok!(Hrmp::poke_channel_deposits(para_origin(2002), 2001.into(), 2002.into()));
		assert_ok!(Hrmp::hrmp_close_channel(para_origin(2002), channel(2001, 2002)));
		assert_ok!(Hrmp::hrmp_accept_open_channel(para_origin(2003), 2001.into()));
		assert_ok!(Hrmp::hrmp_cancel_open_request(para_origin(2003), channel(2003, 2001), 2));
		assert_eq!(counted(2001), 4);
	});
}

/// A system para is not counted: it may make any number of requests.
#[test]
fn system_paras_are_not_counted() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		MaxRequestsPerWindow::set(1);
		for para in [1000, 1999, 2000, 2001] {
			register_parachain(para.into());
		}
		run_to_block(5, Some(vec![4, 5]));

		for system in [1000, 1999] {
			for _ in 0..3 {
				assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(system), 2001.into(), 2, 8));
				assert_ok!(Hrmp::hrmp_cancel_open_request(
					para_origin(system),
					channel(system, 2001),
					1
				));
			}
		}
		assert_eq!(RequestWindows::<Test>::iter().count(), 0);

		// The first para that is not a system one.
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(2000), 2001.into(), 2, 8));
		assert_eq!(counted(2000), 1);
		assert_noop!(
			Hrmp::hrmp_cancel_open_request(para_origin(2000), channel(2000, 2001), 1),
			Error::<Test>::TooManyRequests
		);
	});
}

/// What a signed origin or the `ChannelManager` does to the channels of a para is not counted
/// for it.
#[test]
fn other_origins_are_not_counted() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		MaxRequestsPerWindow::set(1);
		for para in [2001, 2002] {
			register_parachain(para.into());
		}
		run_to_block(5, Some(vec![4, 5]));
		insert_channel_without_deposits(&channel(2001, 2002));

		for _ in 0..2 {
			let (sender, recipient) = (2001.into(), 2002.into());
			assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), sender, recipient));
			assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::root(), sender, recipient));
		}
		assert_ok!(Hrmp::force_open_hrmp_channel(
			RuntimeOrigin::root(),
			2002.into(),
			2001.into(),
			2,
			8
		));
		assert_eq!(RequestWindows::<Test>::iter().count(), 0);
	});
}

/// A request that fails is not counted.
#[test]
fn failed_request_is_not_counted() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		register_parachain(2001.into());
		run_to_block(5, Some(vec![4, 5]));

		assert_noop!(
			Hrmp::hrmp_init_open_channel(para_origin(2001), 2001.into(), 2, 8),
			Error::<Test>::OpenHrmpChannelToSelf
		);
		assert_noop!(
			Hrmp::hrmp_accept_open_channel(para_origin(2001), 2002.into()),
			Error::<Test>::AcceptHrmpChannelDoesntExist
		);
		assert_eq!(window(2001), None);
	});
}

/// A window lasts `RequestWindow` blocks from the first request counted in it. The next one
/// starts at the next request, not right when the previous one ends.
#[test]
fn window_restarts_at_the_next_request() {
	new_test_ext(free_channels_genesis()).execute_with(|| {
		MaxRequestsPerWindow::set(4);
		let len = RequestWindow::get();
		insert_channel_without_deposits(&channel(2001, 2002));
		let request = || Hrmp::poke_channel_deposits(para_origin(2001), 2001.into(), 2002.into());

		System::set_block_number(1);
		for _ in 0..4 {
			assert_ok!(request());
		}
		assert_eq!(window(2001), Some((1, 4)));

		// The window started at block 1: its last block is `len`.
		System::set_block_number(len);
		assert_noop!(request(), Error::<Test>::TooManyRequests);
		System::set_block_number(1 + len);
		assert_ok!(request());
		assert_eq!(window(2001), Some((1 + len, 1)));

		// Long after that window is over, the next request starts a whole new one.
		System::set_block_number(10 * len);
		for _ in 0..4 {
			assert_ok!(request());
		}
		assert_eq!(window(2001), Some((10 * len, 4)));
		System::set_block_number(11 * len - 1);
		assert_noop!(request(), Error::<Test>::TooManyRequests);
		System::set_block_number(11 * len);
		assert_ok!(request());
		assert_eq!(window(2001), Some((11 * len, 1)));
	});
}

/// `on_idle` looks at one window per block and removes it if it is over. Past the last one, it
/// starts over from the first.
#[test]
fn on_idle_removes_the_windows_that_are_over() {
	new_test_ext(Default::default()).execute_with(|| {
		let len = RequestWindow::get();
		let idle = || Hrmp::on_idle(1, Weight::MAX);
		let windows = || RequestWindows::<Test>::iter().count();
		let start = |para: u32, at| RequestWindows::<Test>::insert(ParaId::from(para), (at, 1));

		// Nothing to look at.
		idle();
		assert_eq!(PruneCursor::<Test>::get(), None);

		for para in 2001..2004 {
			start(para, 1);
		}
		// The window of 2004 starts later than the others.
		start(2004, 2);

		// No window is over yet: a whole round removes nothing, then starts over.
		System::set_block_number(len);
		for _ in 0..4 {
			idle();
			assert!(PruneCursor::<Test>::get().is_some());
		}
		idle();
		assert_eq!(PruneCursor::<Test>::get(), None);
		assert_eq!(windows(), 4);

		// All are over but the one of 2004: one is removed per block.
		System::set_block_number(1 + len);
		for looked_at in 1..=4 {
			idle();
			let over = (2001..2004).filter(|para| window(*para).is_none()).count();
			assert!(over == looked_at || over == looked_at - 1);
		}
		assert_eq!(windows(), 1);
		assert_eq!(window(2004), Some((2, 1)));

		// The round ends, and the next one removes the last window once it is over.
		idle();
		assert_eq!(PruneCursor::<Test>::get(), None);
		System::set_block_number(2 + len);
		idle();
		assert_eq!(windows(), 0);
		assert_eq!(PruneCursor::<Test>::get(), Some(ParaId::from(2004)));
		idle();
		assert_eq!(PruneCursor::<Test>::get(), None);
	});
}

/// `on_idle` does nothing without the weight to look at a window.
#[test]
fn on_idle_respects_remaining_weight() {
	new_test_ext(Default::default()).execute_with(|| {
		RequestWindows::<Test>::insert(ParaId::from(2001), (0, 1));
		System::set_block_number(RequestWindow::get());

		let weight = <Test as Config>::WeightInfo::prune_request_window();
		let short = weight.saturating_sub(Weight::from_parts(1, 0));
		assert_eq!(Hrmp::on_idle(1, short), Weight::zero());
		assert_eq!(window(2001), Some((0, 1)));

		assert_eq!(Hrmp::on_idle(1, weight), weight);
		assert_eq!(window(2001), None);
	});
}
