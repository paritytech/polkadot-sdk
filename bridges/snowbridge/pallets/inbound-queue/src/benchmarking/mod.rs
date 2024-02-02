// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::Pallet as InboundQueue;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;
use snowbridge_pallet_inbound_queue_fixtures::register_token::make_register_token_message;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let create_message = make_register_token_message();

		T::Helper::initialize_storage(
			create_message.message.proof.block_hash,
			create_message.execution_header,
		);

		let sovereign_account = sibling_sovereign_account::<T>(1000u32.into());

		let minimum_balance = T::Token::minimum_balance();

		// So that the receiving account exists
		assert_ok!(T::Token::mint_into(&caller, minimum_balance));
		// Fund the sovereign account (parachain sovereign account) so it can transfer a reward
		// fee to the caller account
		assert_ok!(T::Token::mint_into(
			&sovereign_account,
			3_000_000_000_000u128
				.try_into()
				.unwrap_or_else(|_| panic!("unable to cast sovereign account balance")),
		));

		#[block]
		{
			assert_ok!(InboundQueue::<T>::submit(
				RawOrigin::Signed(caller.clone()).into(),
				create_message.message,
			));
		}

		Ok(())
	}

	impl_benchmark_test_suite!(InboundQueue, crate::mock::new_tester(), crate::mock::Test);
}
