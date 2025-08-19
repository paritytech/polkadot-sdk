// See the License for the specific language governing permissions and
// limitations under the License.

//! The pallet-revive shared VM integration test suite.

use crate::{
	test_utils::{builder::Contract, ALICE, ALICE_ADDR},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};
use alloy_core::{
	primitives::{Bytes, FixedBytes, U256},
	sol_types::SolCall,
};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Callee, Caller, FixtureType};
use pretty_assertions::assert_eq;
use sp_core::H160;

/// Tests that the `CALL` opcode works as expected by having one contract call another.
#[test]
fn staticcall_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let _ = sp_tracing::try_init_simple();

		let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Instantiate the callee contract, which can echo a value.
			let Contract { addr: callee_addr, .. } =
				builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

			log::info!("Callee  addr: {:?}", callee_addr);

			// Instantiate the caller contract.
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

			log::info!("Caller  addr: {:?}", caller_addr);

			let magic_number = U256::from(42);
			log::info!("Calling callee from caller");
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::staticCallCall {
						_callee: callee_addr.0.into(),
						_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
						_gas: U256::MAX,
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();
			assert!(result.success, "the call must succeed");
			assert_eq!(
				magic_number,
				U256::from_be_bytes::<32>(result.output.as_ref().try_into().unwrap()),
				"the call must reproduce the magic number"
			);

			// Enable it once sstore host fn is implemented
			// log::info!("Calling callee from caller");
			// let result = builder::bare_call(caller_addr)
			// 	.data(
			// 		Caller::staticCallCall {
			// 			_callee: callee_addr.0.into(),
			// 			_data: Callee::storeCall { _data: magic_number }.abi_encode().into(),
			// 			_gas: U256::MAX,
			// 		}
			// 		.abi_encode(),
			// 	)
			// 	.build_and_unwrap_result();

			// let result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();
			// assert!(!result.success, "Can not store in static call");
		});
	}
}

#[test]
fn call_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let _ = sp_tracing::try_init_simple();

		let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Instantiate the callee contract, which can echo a value.
			let Contract { addr: callee_addr, .. } =
				builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

			log::info!("Callee  addr: {:?}", callee_addr);

			// Instantiate the caller contract.
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

			log::info!("Caller  addr: {:?}", caller_addr);

			let magic_number = U256::from(42);
			log::info!("Calling callee from caller");
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::normalCall {
						_callee: callee_addr.0.into(),
						_value: U256::ZERO,
						_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
						_gas: U256::MAX,
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
			assert!(result.success, "the call must succeed");
			assert_eq!(
				magic_number,
				U256::from_be_bytes::<32>(result.output.as_ref().try_into().unwrap()),
				"the call must reproduce the magic number"
			);

			// Enable it once sstore host fn is implemented
			// log::info!("Calling callee from caller");
			// let result = builder::bare_call(caller_addr)
			// 	.data(
			// 		Caller::normalCall {
			// 			_callee: callee_addr.0.into(),
			// 			_value: U256::ZERO,
			// 			_data: Callee::storeCall { _data: magic_number }.abi_encode().into(),
			// 			_gas: U256::MAX,
			// 		}
			// 		.abi_encode(),
			// 	)
			// 	.build_and_unwrap_result();

			// let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
			// assert!(result.success, "the store call must succeed");
		});
	}
}

#[test]
fn delegatecall_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let _ = sp_tracing::try_init_simple();

		let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Instantiate the callee contract, which can echo a value.
			let Contract { addr: callee_addr, .. } =
				builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

			log::info!("Callee  addr: {:?}", callee_addr);

			// Instantiate the caller contract.
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

			log::info!("Caller  addr: {:?}", caller_addr);

			let magic_number = U256::from(42);
			log::info!("Calling callee.echo() from caller");
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::delegateCall {
						_callee: callee_addr.0.into(),
						_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
						_gas: U256::MAX,
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
			assert!(result.success, "the call must succeed");
			assert_eq!(
				magic_number,
				U256::from_be_bytes::<32>(result.output.as_ref().try_into().unwrap()),
				"the call must reproduce the magic number"
			);

			log::info!("Calling callee.whoSender() from caller");
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::delegateCall {
						_callee: callee_addr.0.into(),
						_data: Callee::whoSenderCall {}.abi_encode().into(),
						_gas: U256::MAX,
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
			assert!(result.success, "the whoSender call must succeed");
			assert_eq!(ALICE_ADDR, H160::from_slice(&result.output.as_ref()[12..]));
		});
	}
}

#[test]
fn create_works() {
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let create_call_data =
			Caller::createCall { initcode: Bytes::from(callee_code.clone()) }.abi_encode();

		let result =
			builder::bare_call(caller_addr).data(create_call_data).build_and_unwrap_result();

		let callee_addr = Caller::createCall::abi_decode_returns(&result.data).unwrap();

		log::info!("Created  addr: {:?}", callee_addr);

		let magic_number = U256::from(42);

		// Check if the created contract is working
		let echo_result = builder::bare_call(callee_addr.0 .0.into())
			.data(Callee::echoCall { _data: magic_number }.abi_encode())
			.build_and_unwrap_result();

		let echo_output = Callee::echoCall::abi_decode_returns(&echo_result.data).unwrap();

		assert_eq!(echo_output, magic_number, "Callee.echo must return 42");
	});
}

#[test]
fn create2_works() {
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let salt = [42u8; 32];

		let initcode = Bytes::from(callee_code);
		// Prepare the CREATE2 call
		let create_call_data =
			Caller::create2Call { initcode: initcode.clone(), salt: FixedBytes(salt) }.abi_encode();

		let result =
			builder::bare_call(caller_addr).data(create_call_data).build_and_unwrap_result();

		let callee_addr = Caller::create2Call::abi_decode_returns(&result.data).unwrap();

		log::info!("Created  addr: {:?}", callee_addr);

		// Compute expected CREATE2 address
		let expected_addr = crate::address::create2(&caller_addr, &initcode, &[], &salt);

		let callee_addr: H160 = callee_addr.0 .0.into();

		assert_eq!(callee_addr, expected_addr, "CREATE2 address should be deterministic");

		let magic_number = U256::from(42);

		// Check if the created contract is working
		let echo_result = builder::bare_call(callee_addr)
			.data(Callee::echoCall { _data: magic_number }.abi_encode())
			.build_and_unwrap_result();

		let echo_output = Callee::echoCall::abi_decode_returns(&echo_result.data).unwrap();

		assert_eq!(echo_output, magic_number, "Callee.echo must return 42");
	});
}
