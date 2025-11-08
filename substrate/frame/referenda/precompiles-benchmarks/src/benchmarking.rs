#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use super::*;
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		Get, OriginTrait, Polling,
	},
};
  use pallet_referenda::BalanceOf;
use scale_info::prelude::collections::BTreeMap;
use sp_runtime::{traits::StaticLookup, Saturating};


use frame_system::RawOrigin;
use pallet_referenda_precompiles::IReferenda;
use pallet_revive::{AddressMapper, ExecConfig, ExecReturnValue, Weight, U256,
	precompiles::alloy::{hex, sol_types::SolInterface},
	H160,
};

use crate::Pallet as ReferendaPrecompilesBenchmarks;
use pallet_referenda::{Pallet as Referenda};

fn call_precompile<T: Config<I>, I: 'static>(
	from: T::AccountId,
	encoded_call: Vec<u8>,
) -> Result<ExecReturnValue, sp_runtime::DispatchError> {
	let precompile_addr = H160::from_low_u64_be(0xB0000);

	let result = pallet_revive::Pallet::<T>::bare_call(
		<T as frame_system::Config>::RuntimeOrigin::signed(from),
		precompile_addr,
		U256::zero(),
		Weight::MAX,
		T::Balance::try_from(U256::from(u128::MAX)).ok().unwrap(),
		encoded_call,
		ExecConfig::new_substrate_tx(),
	);

	return result.result
}

fn funded_mapped_account<T: Config<I>, I: 'static>(name: &'static str, index: u32) -> T::AccountId {
	let account: T::AccountId = account(name, index, 0u32);

	let funding_amount =
		<T as pallet_revive::Config>::Currency::minimum_balance().saturating_mul(100_000u32.into());

	assert_ok!(<T as pallet_revive::Config>::Currency::mint_into(&account, funding_amount));

	assert_ok!(pallet_revive::Pallet::<T>::map_account(RawOrigin::Signed(account.clone()).into()));

	account
}

/*(
	where
	T: crate::Config,
	BalanceOf<T, ()>: TryFrom<u128> + Into<u128>,
    IndexOf<T, ()>: TryFrom<u32> + TryInto<u32>,
    ClassOf<T, ()>: TryFrom<u16> + TryInto<u16>,
) */
#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn submission_deposit() {
		let caller = funded_mapped_account::<T, ()>("caller", 0);
		
		let encoded_call = IReferenda::IReferendaCalls::submissionDeposit(
			IReferenda::submissionDepositCall {},
		)
		.abi_encode();

		let result;
		#[block]
		{
			result = call_precompile::<T, ()>(caller, encoded_call);
		}

		assert!(result.is_ok());
	}

	impl_benchmark_test_suite!(
		ReferendaPrecompilesBenchmarks,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
