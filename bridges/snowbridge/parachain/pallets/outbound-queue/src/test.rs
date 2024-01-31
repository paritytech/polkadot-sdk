// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, *};

use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Hooks, ProcessMessage, ProcessMessageError},
	weights::WeightMeter,
};

use codec::Encode;
use snowbridge_core::{
	outbound::{Command, SendError, SendMessage},
	ParaId, PricingParameters, Rewards,
};
use sp_arithmetic::FixedU128;
use sp_core::H256;
use sp_runtime::FixedPointNumber;

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

		for para_id in 1000..1004 {
			let origin: ParaId = (para_id as u32).into();
			let channel_id: ChannelId = origin.into();
			assert_eq!(Nonce::<Test>::get(channel_id), 1);
		}

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
fn convert_from_ether_decimals() {
	assert_eq!(
		OutboundQueue::convert_from_ether_decimals(1_000_000_000_000_000_000),
		1_000_000_000_000
	);
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

		let channel_id: ChannelId = ParaId::from(1000).into();
		let origin = AggregateMessageOrigin::Snowbridge(channel_id);
		let message = QueuedMessage {
			id: Default::default(),
			channel_id,
			command: Command::Upgrade {
				impl_address: Default::default(),
				impl_code_hash: Default::default(),
				initializer: None,
			},
		}
		.encode();

		let mut meter = WeightMeter::new();

		assert_noop!(
			OutboundQueue::process_message(message.as_slice(), origin, &mut meter, &mut [0u8; 32]),
			ProcessMessageError::Yield
		);
	})
}

#[test]
fn process_message_fails_on_max_nonce_reached() {
	new_tester().execute_with(|| {
		let sibling_id = 1000;
		let channel_id: ChannelId = ParaId::from(sibling_id).into();
		let origin = AggregateMessageOrigin::Snowbridge(channel_id);
		let message: QueuedMessage = QueuedMessage {
			id: H256::zero(),
			channel_id,
			command: mock_message(sibling_id).command,
		};
		let versioned_queued_message: VersionedQueuedMessage = message.try_into().unwrap();
		let encoded = versioned_queued_message.encode();
		let mut meter = WeightMeter::with_limit(Weight::MAX);

		Nonce::<Test>::set(channel_id, u64::MAX);

		assert_noop!(
			OutboundQueue::process_message(encoded.as_slice(), origin, &mut meter, &mut [0u8; 32]),
			ProcessMessageError::Unsupported
		);
	})
}

#[test]
fn process_message_fails_on_overweight_message() {
	new_tester().execute_with(|| {
		let sibling_id = 1000;
		let channel_id: ChannelId = ParaId::from(sibling_id).into();
		let origin = AggregateMessageOrigin::Snowbridge(channel_id);
		let message: QueuedMessage = QueuedMessage {
			id: H256::zero(),
			channel_id,
			command: mock_message(sibling_id).command,
		};
		let versioned_queued_message: VersionedQueuedMessage = message.try_into().unwrap();
		let encoded = versioned_queued_message.encode();
		let mut meter = WeightMeter::with_limit(Weight::from_parts(1, 1));
		assert_noop!(
			OutboundQueue::process_message(encoded.as_slice(), origin, &mut meter, &mut [0u8; 32]),
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
	use snowbridge_core::PRIMARY_GOVERNANCE_CHANNEL;
	use AggregateMessageOrigin::*;

	let sibling_id: u32 = 1000;
	let sibling_channel_id: ChannelId = ParaId::from(sibling_id).into();

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

		let footprint = MessageQueue::footprint(Snowbridge(sibling_channel_id));
		assert_eq!(footprint.storage.count, (max_messages) as u64);

		let message = mock_governance_message::<Test>();
		let (ticket, _) = OutboundQueue::validate(&message).unwrap();
		OutboundQueue::deliver(ticket).unwrap();

		// move to next block
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();

		// first process 20 messages from sibling channel
		let footprint = MessageQueue::footprint(Snowbridge(sibling_channel_id));
		assert_eq!(footprint.storage.count, 40 - 20);

		// and governance message does not have the chance to execute in same block
		let footprint = MessageQueue::footprint(Snowbridge(PRIMARY_GOVERNANCE_CHANNEL));
		assert_eq!(footprint.storage.count, 1);

		// move to next block
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();

		// now governance message get executed in this block
		let footprint = MessageQueue::footprint(Snowbridge(PRIMARY_GOVERNANCE_CHANNEL));
		assert_eq!(footprint.storage.count, 0);

		// and this time process 19 messages from sibling channel so we have 1 message left
		let footprint = MessageQueue::footprint(Snowbridge(sibling_channel_id));
		assert_eq!(footprint.storage.count, 1);

		// move to the next block, the last 1 message from sibling channel get executed
		ServiceWeight::set(Some(Weight::MAX));
		run_to_end_of_next_block();
		let footprint = MessageQueue::footprint(Snowbridge(sibling_channel_id));
		assert_eq!(footprint.storage.count, 0);
	});
}

#[test]
fn convert_local_currency() {
	new_tester().execute_with(|| {
		let fee: u128 = 1_000_000;
		let fee1 = FixedU128::from_inner(fee).into_inner();
		let fee2 = FixedU128::from(fee)
			.into_inner()
			.checked_div(FixedU128::accuracy())
			.expect("accuracy is not zero; qed");
		assert_eq!(fee, fee1);
		assert_eq!(fee, fee2);
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

#[test]
fn validate_messages_with_fees() {
	new_tester().execute_with(|| {
		let message = mock_message(1000);
		let (_, fee) = OutboundQueue::validate(&message).unwrap();
		assert_eq!(fee.local, 698000000);
		assert_eq!(fee.remote, 2680000000000);
	});
}

#[test]
fn test_calculate_fees() {
	new_tester().execute_with(|| {
		let gas_used: u64 = 250000;
		let illegal_price_params: PricingParameters<<Test as Config>::Balance> =
			PricingParameters {
				exchange_rate: FixedU128::from_rational(1, 400),
				fee_per_gas: 10000_u32.into(),
				rewards: Rewards { local: 1_u32.into(), remote: 1_u32.into() },
			};
		let fee = OutboundQueue::calculate_fee(gas_used, illegal_price_params);
		assert_eq!(fee.local, 698000000);
		assert_eq!(fee.remote, 1000000);
	});
}

#[test]
fn test_calculate_fees_with_valid_exchange_rate_but_remote_fee_calculated_as_zero() {
	new_tester().execute_with(|| {
		let gas_used: u64 = 250000;
		let illegal_price_params: PricingParameters<<Test as Config>::Balance> =
			PricingParameters {
				exchange_rate: FixedU128::from_rational(1, 1),
				fee_per_gas: 1_u32.into(),
				rewards: Rewards { local: 1_u32.into(), remote: 1_u32.into() },
			};
		let fee = OutboundQueue::calculate_fee(gas_used, illegal_price_params.clone());
		assert_eq!(fee.local, 698000000);
		// Though none zero pricing params the remote fee calculated here is invalid
		// which should be avoided
		assert_eq!(fee.remote, 0);
	});
}
