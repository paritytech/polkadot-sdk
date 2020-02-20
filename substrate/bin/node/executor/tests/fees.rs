// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Encode, Joiner};
use frame_support::{
	StorageValue, StorageMap,
	traits::Currency,
	weights::GetDispatchInfo,
};
use sp_core::{
	Blake2Hasher, NeverNativeValue, map,
	storage::Storage,
};
use sp_runtime::{
	Fixed64, Perbill,
	traits::Convert,
};
use node_runtime::{
	CheckedExtrinsic, Call, Runtime, Balances, TransactionPayment, TransactionBaseFee,
	TransactionByteFee, WeightFeeCoefficient,
	constants::currency::*,
};
use node_runtime::impls::LinearWeightToFee;
use node_primitives::Balance;
use node_testing::keyring::*;

pub mod common;
use self::common::{*, sign};

#[test]
fn fee_multiplier_increases_and_decreases_on_big_weight() {
	let mut t = new_test_ext(COMPACT_CODE, false);

	// initial fee multiplier must be zero
	let mut prev_multiplier = Fixed64::from_parts(0);

	t.execute_with(|| {
		assert_eq!(TransactionPayment::next_fee_multiplier(), prev_multiplier);
	});

	let mut tt = new_test_ext(COMPACT_CODE, false);

	// big one in terms of weight.
	let block1 = construct_block(
		&mut tt,
		1,
		GENESIS_HASH.into(),
		vec![
			CheckedExtrinsic {
				signed: None,
				function: Call::Timestamp(pallet_timestamp::Call::set(42 * 1000)),
			},
			CheckedExtrinsic {
				signed: Some((charlie(), signed_extra(0, 0))),
				function: Call::System(frame_system::Call::fill_block(Perbill::from_percent(90))),
			}
		]
	);

	// small one in terms of weight.
	let block2 = construct_block(
		&mut tt,
		2,
		block1.1.clone(),
		vec![
			CheckedExtrinsic {
				signed: None,
				function: Call::Timestamp(pallet_timestamp::Call::set(52 * 1000)),
			},
			CheckedExtrinsic {
				signed: Some((charlie(), signed_extra(1, 0))),
				function: Call::System(frame_system::Call::remark(vec![0; 1])),
			}
		]
	);

	println!(
		"++ Block 1 size: {} / Block 2 size {}",
		block1.0.encode().len(),
		block2.0.encode().len(),
	);

	// execute a big block.
	executor_call::<NeverNativeValue, fn() -> _>(
		&mut t,
		"Core_execute_block",
		&block1.0,
		true,
		None,
	).0.unwrap();

	// weight multiplier is increased for next block.
	t.execute_with(|| {
		let fm = TransactionPayment::next_fee_multiplier();
		println!("After a big block: {:?} -> {:?}", prev_multiplier, fm);
		assert!(fm > prev_multiplier);
		prev_multiplier = fm;
	});

	// execute a big block.
	executor_call::<NeverNativeValue, fn() -> _>(
		&mut t,
		"Core_execute_block",
		&block2.0,
		true,
		None,
	).0.unwrap();

	// weight multiplier is increased for next block.
	t.execute_with(|| {
		let fm = TransactionPayment::next_fee_multiplier();
		println!("After a small block: {:?} -> {:?}", prev_multiplier, fm);
		assert!(fm < prev_multiplier);
	});
}

#[test]
fn transaction_fee_is_correct_ultimate() {
	// This uses the exact values of substrate-node.
	//
	// weight of transfer call as of now: 1_000_000
	// if weight of the cheapest weight would be 10^7, this would be 10^9, which is:
	//   - 1 MILLICENTS in substrate node.
	//   - 1 milli-dot based on current polkadot runtime.
	// (this baed on assigning 0.1 CENT to the cheapest tx with `weight = 100`)
	let mut t = TestExternalities::<Blake2Hasher>::new_with_code(COMPACT_CODE, Storage {
		top: map![
			<frame_system::Account<Runtime>>::hashed_key_for(alice()) => {
				(0u32, 100 * DOLLARS, 0 * DOLLARS, 0 * DOLLARS, 0 * DOLLARS).encode()
			},
			<frame_system::Account<Runtime>>::hashed_key_for(bob()) => {
				(0u32, 10 * DOLLARS, 0 * DOLLARS, 0 * DOLLARS, 0 * DOLLARS).encode()
			},
			<pallet_balances::TotalIssuance<Runtime>>::hashed_key().to_vec() => {
				(110 * DOLLARS).encode()
			},
			<frame_system::BlockHash<Runtime>>::hashed_key_for(0) => vec![0u8; 32]
		],
		children: map![],
	});

	let tip = 1_000_000;
	let xt = sign(CheckedExtrinsic {
		signed: Some((alice(), signed_extra(0, tip))),
		function: Call::Balances(default_transfer_call()),
	});

	let r = executor_call::<NeverNativeValue, fn() -> _>(
		&mut t,
		"Core_initialize_block",
		&vec![].and(&from_block_number(1u32)),
		true,
		None,
	).0;

	assert!(r.is_ok());
	let r = executor_call::<NeverNativeValue, fn() -> _>(
		&mut t,
		"BlockBuilder_apply_extrinsic",
		&vec![].and(&xt.clone()),
		true,
		None,
	).0;
	assert!(r.is_ok());

	t.execute_with(|| {
		assert_eq!(Balances::total_balance(&bob()), (10 + 69) * DOLLARS);
		// Components deducted from alice's balances:
		// - Weight fee
		// - Length fee
		// - Tip
		// - Creation-fee of bob's account.
		let mut balance_alice = (100 - 69) * DOLLARS;

		let length_fee = TransactionBaseFee::get() +
			TransactionByteFee::get() *
			(xt.clone().encode().len() as Balance);
		balance_alice -= length_fee;

		let weight = default_transfer_call().get_dispatch_info().weight;
		let weight_fee = LinearWeightToFee::<WeightFeeCoefficient>::convert(weight);

		// we know that weight to fee multiplier is effect-less in block 1.
		assert_eq!(weight_fee as Balance, MILLICENTS);
		balance_alice -= weight_fee;
		balance_alice -= tip;

		assert_eq!(Balances::total_balance(&alice()), balance_alice);
	});
}

#[test]
#[should_panic]
#[cfg(feature = "stress-test")]
fn block_weight_capacity_report() {
	// Just report how many transfer calls you could fit into a block. The number should at least
	// be a few hundred (250 at the time of writing but can change over time). Runs until panic.
	use node_primitives::Index;

	// execution ext.
	let mut t = new_test_ext(COMPACT_CODE, false);
	// setup ext.
	let mut tt = new_test_ext(COMPACT_CODE, false);

	let factor = 50;
	let mut time = 10;
	let mut nonce: Index = 0;
	let mut block_number = 1;
	let mut previous_hash: Hash = GENESIS_HASH.into();

	loop {
		let num_transfers = block_number * factor;
		let mut xts = (0..num_transfers).map(|i| CheckedExtrinsic {
			signed: Some((charlie(), signed_extra(nonce + i as Index, 0))),
			function: Call::Balances(pallet_balances::Call::transfer(bob().into(), 0)),
		}).collect::<Vec<CheckedExtrinsic>>();

		xts.insert(0, CheckedExtrinsic {
			signed: None,
			function: Call::Timestamp(pallet_timestamp::Call::set(time * 1000)),
		});

		// NOTE: this is super slow. Can probably be improved.
		let block = construct_block(
			&mut tt,
			block_number,
			previous_hash,
			xts
		);

		let len = block.0.len();
		print!(
			"++ Executing block with {} transfers. Block size = {} bytes / {} kb / {} mb",
			num_transfers,
			len,
			len / 1024,
			len / 1024 / 1024,
		);

		let r = executor_call::<NeverNativeValue, fn() -> _>(
			&mut t,
			"Core_execute_block",
			&block.0,
			true,
			None,
		).0;

		println!(" || Result = {:?}", r);
		assert!(r.is_ok());

		previous_hash = block.1;
		nonce += num_transfers;
		time += 10;
		block_number += 1;
	}
}

#[test]
#[should_panic]
#[cfg(feature = "stress-test")]
fn block_length_capacity_report() {
	// Just report how big a block can get. Executes until panic. Should be ignored unless if
	// manually inspected. The number should at least be a few megabytes (5 at the time of
	// writing but can change over time).
	use node_primitives::Index;

	// execution ext.
	let mut t = new_test_ext(COMPACT_CODE, false);
	// setup ext.
	let mut tt = new_test_ext(COMPACT_CODE, false);

	let factor = 256 * 1024;
	let mut time = 10;
	let mut nonce: Index = 0;
	let mut block_number = 1;
	let mut previous_hash: Hash = GENESIS_HASH.into();

	loop {
		// NOTE: this is super slow. Can probably be improved.
		let block = construct_block(
			&mut tt,
			block_number,
			previous_hash,
			vec![
				CheckedExtrinsic {
					signed: None,
					function: Call::Timestamp(pallet_timestamp::Call::set(time * 1000)),
				},
				CheckedExtrinsic {
					signed: Some((charlie(), signed_extra(nonce, 0))),
					function: Call::System(frame_system::Call::remark(vec![0u8; (block_number * factor) as usize])),
				},
			]
		);

		let len = block.0.len();
		print!(
			"++ Executing block with big remark. Block size = {} bytes / {} kb / {} mb",
			len,
			len / 1024,
			len / 1024 / 1024,
		);

		let r = executor_call::<NeverNativeValue, fn() -> _>(
			&mut t,
			"Core_execute_block",
			&block.0,
			true,
			None,
		).0;

		println!(" || Result = {:?}", r);
		assert!(r.is_ok());

		previous_hash = block.1;
		nonce += 1;
		time += 10;
		block_number += 1;
	}
}
