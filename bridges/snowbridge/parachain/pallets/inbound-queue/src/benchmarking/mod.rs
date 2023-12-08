mod fixtures;

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::Pallet as InboundQueue;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_std::vec;

#[benchmarks]
mod benchmarks {
	use super::*;
	use crate::benchmarking::fixtures::make_create_message;

	#[benchmark]
	fn submit() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let create_message = make_create_message();

		T::Helper::initialize_storage(
			create_message.message.proof.block_hash,
			create_message.execution_header,
		);

		let sovereign_account = sibling_sovereign_account::<T>(1000u32.into());

		let minimum_balance = T::Token::minimum_balance();
		let minimum_balance_u32: u32 = minimum_balance
			.try_into()
			.unwrap_or_else(|_| panic!("unable to cast minimum balance to u32"));

		// Make sure the sovereign balance is enough. This is a funny number, because
		// in some cases the minimum balance is really high, in other cases very low.
		// e.g. on bridgehub the minimum balance is 33333, on test it is 1. So this equation makes
		// it is at least twice the minimum balance (so as to satisfy the minimum balance
		// requirement, and then some (in case the minimum balance is very low, even lower
		// than the relayer reward fee).
		let sovereign_balance = (minimum_balance_u32 * 2) + 5000;

		// So that the receiving account exists
		let _ = T::Token::mint_into(&caller, minimum_balance.into());
		// Fund the sovereign account (parachain sovereign account) so it can transfer a reward
		// fee to the caller account
		let _ = T::Token::mint_into(&sovereign_account, sovereign_balance.into());

		#[block]
		{
			let _ = InboundQueue::<T>::submit(
				RawOrigin::Signed(caller.clone()).into(),
				create_message.message,
			)?;
		}

		Ok(())
	}

	impl_benchmark_test_suite!(InboundQueue, crate::mock::new_tester(), crate::mock::Test);
}
