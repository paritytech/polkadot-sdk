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
		answer_hold, deregister_parachain, new_test_ext, register_parachain,
		register_parachain_with_balance, AskedHolds, AsyncDeposits, Dmp, Hrmp, MockGenesisConfig,
		Paras, ParasShared, RemoteHeld, RuntimeEvent as MockEvent, RuntimeOrigin, System, Test,
		TestUsesOnlyStoredVersionWrapper,
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
				Dmp::dmq_contents(para_id),
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

fn hrmp_events() -> Vec<Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			MockEvent::Hrmp(e) => Some(e),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}

fn free(para: ParaId) -> Balance {
	<Test as Config>::Currency::free_balance(&para.into_account_truncating())
}

fn reserved(para: ParaId) -> Balance {
	<Test as Config>::Currency::reserved_balance(&para.into_account_truncating())
}

/// Sender deposit 20, recipient deposit 15.
fn async_genesis() -> MockGenesisConfig {
	let mut genesis = GenesisConfigBuilder::default();
	genesis.hrmp_sender_deposit = 20;
	genesis.hrmp_recipient_deposit = 15;
	genesis.build()
}

/// Register `paras` with 100 each, onboard them, then hold deposits asynchronously.
fn async_setup(paras: &[ParaId]) {
	for para in paras {
		register_parachain_with_balance(*para, 100);
	}
	run_to_block(5, Some(vec![4, 5]));
	AsyncDeposits::set(true);
	AskedHolds::set(vec![]);
	RemoteHeld::set(Default::default());
	let _ = hrmp_events();
}

#[test]
fn async_deposits_open_and_close_a_channel_without_relay_funds() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };
	let sender_key = DepositKey::sender(channel.clone());
	let recipient_key = DepositKey::recipient(channel.clone());

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN deposits are held on another chain.
		async_setup(&[alice, bob]);

		// WHEN alice asks for a channel
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));

		// THEN only the hold goes out: no request, no notification.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert_eq!(
			PendingDeposits::<Test>::get(&sender_key),
			Some(PendingDeposit::Init {
				max_capacity: 2,
				max_message_size: 8,
				amount: 20,
				then_accept: false,
			})
		);
		assert_eq!(AskedHolds::get(), vec![(sender_key.clone(), 20)]);
		assert!(Dmp::dmq_contents(bob).is_empty());

		// WHEN the hold lands
		answer_hold(true);

		// THEN the request is written and bob is told.
		assert_eq!(HrmpOpenChannelRequests::<Test>::get(&channel).unwrap().sender_deposit, 20);
		assert_eq!(Dmp::dmq_contents(bob).len(), 1);
		assert_eq!(PendingDeposits::<Test>::iter().count(), 0);

		// WHEN bob accepts, and the hold lands
		assert_ok!(Hrmp::accept_open_channel(bob, alice));
		assert!(!HrmpOpenChannelRequests::<Test>::get(&channel).unwrap().confirmed);
		assert_eq!(Dmp::dmq_contents(alice).len(), 0);
		answer_hold(true);

		// THEN the request is confirmed and alice is told.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).unwrap().confirmed);
		assert_eq!(Dmp::dmq_contents(alice).len(), 1);

		// WHEN the session turns
		run_to_block(8, Some(vec![8]));

		// THEN the channel opens with nothing reserved here, and the deposits are held remotely.
		assert!(channel_exists(alice, bob));
		assert_eq!((free(alice), reserved(alice)), (100, 0));
		assert_eq!((free(bob), reserved(bob)), (100, 0));
		assert_eq!(
			RemoteHeld::get(),
			BTreeMap::from([(sender_key.clone(), 20), (recipient_key.clone(), 15)])
		);
		Hrmp::assert_storage_consistency_exhaustive();

		// WHEN bob closes and the session turns
		assert_ok!(Hrmp::close_channel(bob, channel.clone()));
		run_to_block(10, Some(vec![10]));

		// THEN both deposits are released remotely.
		assert!(!channel_exists(alice, bob));
		assert!(RemoteHeld::get().is_empty());
		assert_eq!(
			hrmp_events(),
			vec![
				Event::DepositPending { key: sender_key },
				Event::OpenChannelRequested {
					sender: alice,
					recipient: bob,
					proposed_max_capacity: 2,
					proposed_max_message_size: 8,
				},
				Event::DepositPending { key: recipient_key },
				Event::OpenChannelAccepted { sender: alice, recipient: bob },
			]
		);
	});
}

#[test]
fn a_refused_hold_drops_the_request() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };
	let sender_key = DepositKey::sender(channel.clone());

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN alice's request waits on its deposit.
		async_setup(&[alice, bob]);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));

		// WHEN a second request for the same channel arrives meanwhile
		// THEN it is refused.
		assert_noop!(Hrmp::init_open_channel(alice, bob, 2, 8), Error::<Test>::DepositPending);

		// WHEN the hold is refused
		answer_hold(false);

		// THEN nothing is recorded, nothing is held, and alice may ask again.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert_eq!(PendingDeposits::<Test>::iter().count(), 0);
		assert!(RemoteHeld::get().is_empty());
		assert!(Dmp::dmq_contents(bob).is_empty());
		assert_eq!(
			hrmp_events(),
			vec![
				Event::DepositPending { key: sender_key.clone() },
				Event::DepositRefused { key: sender_key },
			]
		);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));
	});
}

#[test]
fn a_held_deposit_is_returned_if_the_request_no_longer_passes() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let charlie: ParaId = 2096.into(); // recipient
	let dave: ParaId = 2128.into(); // recipient
									// The default genesis allows two outbound channels per para.
	let third = HrmpChannelId { sender: alice, recipient: dave };

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN deposits are held on another chain.
		async_setup(&[alice, bob, charlie, dave]);

		// WHEN alice asks for three channels while the first hold is still out
		// THEN all three are parked: a parked request does not count towards the limit.
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));
		assert_ok!(Hrmp::init_open_channel(alice, charlie, 2, 8));
		assert_ok!(Hrmp::init_open_channel(alice, dave, 2, 8));
		assert_eq!(AskedHolds::get().len(), 3);

		// WHEN all three holds land
		answer_hold(true);
		answer_hold(true);
		let _ = hrmp_events();
		let key = answer_hold(true);

		// THEN the third fails its re-check and its deposit goes back.
		assert_eq!(key, DepositKey::sender(third.clone()));
		assert_eq!(HrmpOpenChannelRequestCount::<Test>::get(alice), 2);
		assert!(HrmpOpenChannelRequests::<Test>::get(&third).is_none());
		assert_eq!(RemoteHeld::get().get(&key), None);
		assert_eq!(
			hrmp_events(),
			vec![Event::DepositReturned {
				key,
				error: Error::<Test>::OpenHrmpChannelLimitExceeded.into(),
			}]
		);
	});
}

#[test]
fn a_held_deposit_is_returned_if_the_recipient_offboarded_meanwhile() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient, offboards
	let channel = HrmpChannelId { sender: alice, recipient: bob };

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN alice's request waits on its deposit.
		async_setup(&[alice, bob]);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));

		// WHEN bob offboards before the hold lands
		deregister_parachain(bob);
		run_to_block(7, Some(vec![6, 7]));
		let _ = hrmp_events();
		let key = answer_hold(true);

		// THEN the request is refused and the deposit goes back.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert!(RemoteHeld::get().is_empty());
		assert_eq!(
			hrmp_events(),
			vec![Event::DepositReturned {
				key,
				error: Error::<Test>::OpenHrmpChannelInvalidRecipient.into(),
			}]
		);
	});
}

#[test]
fn an_offboarded_senders_request_deposit_is_released_where_it_is_held() {
	let alice: ParaId = 2032.into(); // sender, offboards
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN alice's request is written and its deposit held elsewhere.
		async_setup(&[alice, bob]);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));
		answer_hold(true);
		assert_eq!(RemoteHeld::get().len(), 1);

		// WHEN alice offboards
		deregister_parachain(alice);
		run_to_block(7, Some(vec![6, 7]));

		// THEN the request is gone and so is the deposit. Reserved locally it would stay reserved,
		// which `refund_deposit_on_offboarding` pins for `ReserveDeposits`.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert!(RemoteHeld::get().is_empty());
	});
}

#[test]
fn ordered_answers_keep_a_pending_accept_off_a_replaced_request() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };
	let recipient_key = DepositKey::recipient(channel.clone());

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN alice's request for capacity 1 is written, and bob's accept waits on its deposit.
		async_setup(&[alice, bob]);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 1, 8));
		answer_hold(true);
		assert_ok!(Hrmp::accept_open_channel(bob, alice));

		// WHEN alice cancels and asks again with capacity 2 before bob's hold lands
		assert_ok!(Hrmp::cancel_open_request(alice, channel.clone()));
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));
		let _ = hrmp_events();

		// THEN bob's answer, which comes back first, finds no request and returns his deposit,
		let key = answer_hold(true);
		assert_eq!(key, recipient_key);
		assert_eq!(
			hrmp_events(),
			vec![Event::DepositReturned {
				key: recipient_key.clone(),
				error: Error::<Test>::AcceptHrmpChannelDoesntExist.into(),
			}]
		);

		// AND alice's new request lands unconfirmed.
		answer_hold(true);
		let request = HrmpOpenChannelRequests::<Test>::get(&channel).unwrap();
		assert_eq!((request.max_capacity, request.confirmed), (2, false));
		assert_eq!(RemoteHeld::get().get(&recipient_key), None);
	});
}

#[test]
fn force_open_takes_both_deposits_and_accepts_once_the_first_is_held() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };
	let sender_key = DepositKey::sender(channel.clone());
	let recipient_key = DepositKey::recipient(channel.clone());

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN deposits are held on another chain.
		async_setup(&[alice, bob]);

		// WHEN governance force-opens a paying channel
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), alice, bob, 2, 8));

		// THEN it succeeds, waiting on the sender's deposit.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert_eq!(AskedHolds::get(), vec![(sender_key.clone(), 20)]);
		assert_eq!(hrmp_events(), vec![Event::DepositPending { key: sender_key.clone() }]);

		// WHEN the sender's deposit is held
		answer_hold(true);

		// THEN the request is recorded and accepted on the recipient's behalf, which waits on the
		// recipient's deposit.
		assert!(!HrmpOpenChannelRequests::<Test>::get(&channel).unwrap().confirmed);
		assert_eq!(AskedHolds::get(), vec![(recipient_key.clone(), 15)]);

		// WHEN the recipient's deposit is held
		answer_hold(true);

		// THEN the request is confirmed with both deposits held, and opens at the next session.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).unwrap().confirmed);
		assert_eq!(RemoteHeld::get(), BTreeMap::from([(sender_key, 20), (recipient_key, 15)]));
		run_to_block(8, Some(vec![8]));
		assert!(channel_exists(alice, bob));
	});
}

#[test]
fn a_force_open_the_recipient_cannot_accept_returns_the_sender_deposit() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient, at its inbound limit
	let charlie: ParaId = 2096.into(); // sender
	let dave: ParaId = 2128.into(); // sender
	let channel = HrmpChannelId { sender: alice, recipient: bob };

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN bob has as many inbound channels as the default genesis allows (two).
		register_parachain_with_balance(alice, 100);
		register_parachain_with_balance(bob, 100);
		register_parachain_with_balance(charlie, 100);
		register_parachain_with_balance(dave, 100);
		run_to_block(5, Some(vec![4, 5]));
		for sender in [charlie, dave] {
			assert_ok!(Hrmp::init_open_channel(sender, bob, 2, 8));
			assert_ok!(Hrmp::accept_open_channel(bob, sender));
		}
		run_to_block(8, Some(vec![8]));
		AsyncDeposits::set(true);
		let _ = hrmp_events();

		// WHEN governance force-opens alice to bob, and alice's deposit is held
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), alice, bob, 2, 8));
		answer_hold(true);

		// THEN the accept is refused, so the request is withdrawn and alice's deposit released.
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel).is_none());
		assert!(RemoteHeld::get().is_empty());
		assert!(AskedHolds::get().is_empty());
		assert!(hrmp_events().contains(&Event::DepositReturned {
			key: DepositKey::sender(channel.clone()),
			error: Error::<Test>::AcceptHrmpChannelLimitExceeded.into(),
		}));
	});
}

#[test]
fn poke_reprices_deposits_that_are_held_asynchronously() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let channel = HrmpChannelId { sender: alice, recipient: bob };
	let sender_key = DepositKey::sender(channel.clone());
	let recipient_key = DepositKey::recipient(channel.clone());

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN an open channel whose deposits (20 and 15) are held remotely.
		async_setup(&[alice, bob]);
		assert_ok!(Hrmp::init_open_channel(alice, bob, 2, 8));
		answer_hold(true);
		assert_ok!(Hrmp::accept_open_channel(bob, alice));
		answer_hold(true);
		run_to_block(8, Some(vec![8]));
		let _ = hrmp_events();

		// WHEN the sender deposit rises to 30 and the recipient's falls to 5, and someone pokes
		configuration::ActiveConfig::<Test>::mutate(|c| {
			c.hrmp_sender_deposit = 30;
			c.hrmp_recipient_deposit = 5;
		});
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), alice, bob));

		// THEN the decrease is released now, and the increase waits on its hold.
		let recorded = HrmpChannels::<Test>::get(&channel).unwrap();
		assert_eq!((recorded.sender_deposit, recorded.recipient_deposit), (20, 5));
		assert_eq!(RemoteHeld::get().get(&recipient_key), Some(&5));
		assert_eq!(AskedHolds::get(), vec![(sender_key.clone(), 10)]);

		// AND poking again meanwhile is refused.
		assert_noop!(
			Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), alice, bob),
			Error::<Test>::DepositPending
		);

		// WHEN the increase is held
		answer_hold(true);

		// THEN the channel records the new price, and the remote side holds it.
		let recorded = HrmpChannels::<Test>::get(&channel).unwrap();
		assert_eq!((recorded.sender_deposit, recorded.recipient_deposit), (30, 5));
		assert_eq!(
			RemoteHeld::get(),
			BTreeMap::from([(sender_key.clone(), 30), (recipient_key, 5)])
		);

		// WHEN it rises again but the hold is refused
		configuration::ActiveConfig::<Test>::mutate(|c| c.hrmp_sender_deposit = 90);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), alice, bob));
		answer_hold(false);

		// THEN the old deposit stands.
		assert_eq!(HrmpChannels::<Test>::get(&channel).unwrap().sender_deposit, 30);
		assert_eq!(RemoteHeld::get().get(&sender_key), Some(&30));
	});
}

#[test]
fn a_parked_request_is_announced_once_its_deposit_is_held() {
	let alice: ParaId = 2032.into(); // sender
	let bob: ParaId = 2064.into(); // recipient
	let sender_key = DepositKey::sender(HrmpChannelId { sender: alice, recipient: bob });

	new_test_ext(async_genesis()).execute_with(|| {
		// GIVEN deposits are held on another chain.
		async_setup(&[alice, bob]);

		// WHEN alice asks through the extrinsic
		assert_ok!(Hrmp::hrmp_init_open_channel(crate::Origin::Parachain(alice).into(), bob, 2, 8));

		// THEN only the pending deposit is announced,
		assert_eq!(hrmp_events(), vec![Event::DepositPending { key: sender_key.clone() }]);

		// AND the request is announced once the deposit is held.
		answer_hold(true);
		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelRequested {
				sender: alice,
				recipient: bob,
				proposed_max_capacity: 2,
				proposed_max_message_size: 8,
			}]
		);
	});
}
