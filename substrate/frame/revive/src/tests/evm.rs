use self::test_utils::ensure_stored;
use super::{ExtBuilder, Test};
use crate::{
	test_utils::{builder::Contract, *},
	tests::{
		builder,
		test_utils::{self},
		System,
	},
	Code, Config,
};
use frame_support::traits::fungible::Mutate;
use pretty_assertions::assert_eq;
use sp_core::U256;

alloy_core::sol!("src/tests/playground.sol");

#[test]
fn basic_evm_flow_works() {
	use alloy_core::{hex, primitives, sol_types::SolInterface};
	let code = hex::decode(include_str!("Playground.bin")).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		// check the code exists
		let contract = test_utils::get_contract_checked(&addr).unwrap();
		ensure_stored(contract.code_hash);

		let result = builder::bare_call(addr)
			.data(
				Playground::PlaygroundCalls::fib(Playground::fibCall {
					n: primitives::U256::from(10u64),
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert_eq!(U256::from(55u32), U256::from_big_endian(&result.data));
	});
}

#[test]
fn basic_evm_host_interaction_works() {
	use alloy_core::{hex, sol_types::SolInterface};
	let code = hex::decode(include_str!("Playground.bin")).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		System::set_block_number(42);

		let result = builder::bare_call(addr)
			.data(Playground::PlaygroundCalls::bn(Playground::bnCall {}).abi_encode())
			.build_and_unwrap_result();
		assert_eq!(U256::from(42u32), U256::from_big_endian(&result.data));
	});
}
