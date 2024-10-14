// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, *};

use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Hooks, ProcessMessage, ProcessMessageError},
	weights::WeightMeter,
	BoundedVec,
};

use codec::Encode;
use snowbridge_core::{
	outbound_v2::{Command, SendError, SendMessage},
	primary_governance_origin, ChannelId, ParaId,
};
use sp_core::H256;

#[test]
fn submit_messages_and_commit() {
	new_tester().execute_with(|| {
		for para_id in 1000..1004 {
			let message = mock_message(para_id);
			let (ticket, _) = OutboundQueue::validate(&message).unwrap();
			assert_ok!(OutboundQueue::deliver(ticket));
		}

		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();

		assert_eq!(Nonce::<Test>::get(), 4);

		let digest = System::digest();
		let digest_items = digest.logs();
		assert!(digest_items.len() == 1 && digest_items[0].as_other().is_some());
		assert_eq!(Messages::<Test>::decode_len(), Some(4));
	});
}

#[test]
fn submit_message_fail_too_large() {
	new_tester().execute_with(|| {
		let message = mock_invalid_governance_message::<Test>();
		assert_err!(OutboundQueue::validate(&message), SendError::MessageTooLarge);
	});
}

#[test]
fn commit_exits_early_if_no_processed_messages() {
	new_tester().execute_with(|| {
		// on_finalize should do nothing, nor should it panic
		OutboundQueue::on_finalize(System::block_number());

		let digest = System::digest();
		let digest_items = digest.logs();
		assert_eq!(digest_items.len(), 0);
	});
}

#[test]
fn process_message_yields_on_max_messages_per_block() {
	new_tester().execute_with(|| {
		for _ in 0..<Test as Config>::MaxMessagesPerBlock::get() {
			MessageLeaves::<Test>::append(H256::zero())
		}

		let _channel_id: ChannelId = ParaId::from(1000).into();
		let origin = AggregateMessageOrigin::SnowbridgeV2(H256::zero());
		let message = Message {
			origin: Default::default(),
			id: Default::default(),
			fee: 0,
			commands: BoundedVec::try_from(vec![Command::Upgrade {
				impl_address: Default::default(),
				impl_code_hash: Default::default(),
				initializer: None,
			}])
			.unwrap(),
		};

		let mut meter = WeightMeter::new();

		assert_noop!(
			OutboundQueue::process_message(
				message.encode().as_slice(),
				origin,
				&mut meter,
				&mut [0u8; 32]
			),
			ProcessMessageError::Yield
		);
	})
}

#[test]
fn process_message_fails_on_max_nonce_reached() {
	new_tester().execute_with(|| {
		let sibling_id = 1000;
		let _channel_id: ChannelId = ParaId::from(sibling_id).into();
		let origin = AggregateMessageOrigin::SnowbridgeV2(H256::zero());
		let message: Message = mock_message(sibling_id);

		let mut meter = WeightMeter::with_limit(Weight::MAX);

		Nonce::<Test>::set(u64::MAX);

		let result = OutboundQueue::process_message(
			message.encode().as_slice(),
			origin,
			&mut meter,
			&mut [0u8; 32],
		);
		assert_err!(result, ProcessMessageError::Unsupported)
	})
}

#[test]
fn process_message_fails_on_overweight_message() {
	new_tester().execute_with(|| {
		let sibling_id = 1000;
		let _channel_id: ChannelId = ParaId::from(sibling_id).into();
		let origin = AggregateMessageOrigin::SnowbridgeV2(H256::zero());
		let message: Message = mock_message(sibling_id);
		let mut meter = WeightMeter::with_limit(Weight::from_parts(1, 1));
		assert_noop!(
			OutboundQueue::process_message(
				message.encode().as_slice(),
				origin,
				&mut meter,
				&mut [0u8; 32]
			),
			ProcessMessageError::Overweight(<Test as Config>::WeightInfo::do_process_message())
		);
	})
}

// Governance messages should be able to bypass a halted operating mode
// Other message sends should fail when halted
#[test]
fn submit_upgrade_message_success_when_queue_halted() {
	new_tester().execute_with(|| {
		// halt the outbound queue
		OutboundQueue::set_operating_mode(RuntimeOrigin::root(), BasicOperatingMode::Halted)
			.unwrap();

		// submit a high priority message from bridge_hub should success
		let message = mock_governance_message::<Test>();
		let (ticket, _) = OutboundQueue::validate(&message).unwrap();
		assert_ok!(OutboundQueue::deliver(ticket));

		// submit a low priority message from asset_hub will fail as pallet is halted
		let message = mock_message(1000);
		let (ticket, _) = OutboundQueue::validate(&message).unwrap();
		assert_noop!(OutboundQueue::deliver(ticket), SendError::Halted);
	});
}

#[test]
fn governance_message_does_not_get_the_chance_to_processed_in_same_block_when_congest_of_low_priority_sibling_messages(
) {
	use AggregateMessageOrigin::*;

	let sibling_id: u32 = 1000;

	new_tester().execute_with(|| {
		// submit a lot of low priority messages from asset_hub which will need multiple blocks to
		// execute(20 messages for each block so 40 required at least 2 blocks)
		let max_messages = 40;
		for _ in 0..max_messages {
			// submit low priority message
			let message = mock_message(sibling_id);
			let (ticket, _) = OutboundQueue::validate(&message).unwrap();
			OutboundQueue::deliver(ticket).unwrap();
		}

		let footprint =
			MessageQueue::footprint(SnowbridgeV2(H256::from_low_u64_be(sibling_id as u64)));
		assert_eq!(footprint.storage.count, (max_messages) as u64);

		let message = mock_governance_message::<Test>();
		let (ticket, _) = OutboundQueue::validate(&message).unwrap();
		OutboundQueue::deliver(ticket).unwrap();

		// move to next block
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();

		// first process 20 messages from sibling channel
		let footprint =
			MessageQueue::footprint(SnowbridgeV2(H256::from_low_u64_be(sibling_id as u64)));
		assert_eq!(footprint.storage.count, 40 - 20);

		// and governance message does not have the chance to execute in same block
		let footprint = MessageQueue::footprint(SnowbridgeV2(primary_governance_origin()));
		assert_eq!(footprint.storage.count, 1);

		// move to next block
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();

		// now governance message get executed in this block
		let footprint = MessageQueue::footprint(SnowbridgeV2(primary_governance_origin()));
		assert_eq!(footprint.storage.count, 0);

		// and this time process 19 messages from sibling channel so we have 1 message left
		let footprint =
			MessageQueue::footprint(SnowbridgeV2(H256::from_low_u64_be(sibling_id as u64)));
		assert_eq!(footprint.storage.count, 1);

		// move to the next block, the last 1 message from sibling channel get executed
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();
		let footprint =
			MessageQueue::footprint(SnowbridgeV2(H256::from_low_u64_be(sibling_id as u64)));
		assert_eq!(footprint.storage.count, 0);
	});
}

#[test]
fn encode_digest_item_with_correct_index() {
	new_tester().execute_with(|| {
		let digest_item: DigestItem = CustomDigestItem::Snowbridge(H256::default()).into();
		let enum_prefix = match digest_item {
			DigestItem::Other(data) => data[0],
			_ => u8::MAX,
		};
		assert_eq!(enum_prefix, 0);
	});
}

#[test]
fn encode_digest_item() {
	new_tester().execute_with(|| {
		let digest_item: DigestItem = CustomDigestItem::Snowbridge([5u8; 32].into()).into();
		let digest_item_raw = digest_item.encode();
		assert_eq!(digest_item_raw[0], 0); // DigestItem::Other
		assert_eq!(digest_item_raw[2], 0); // CustomDigestItem::Snowbridge
		assert_eq!(
			digest_item_raw,
			[
				0, 132, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
				5, 5, 5, 5, 5, 5, 5, 5
			]
		);
	});
}
