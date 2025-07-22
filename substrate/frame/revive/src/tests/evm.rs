use self::test_utils::ensure_stored;
use super::{ExtBuilder, Test};
use crate::{
	self as pallet_revive,
	address::{create1, create2, AddressMapper},
	assert_refcount, assert_return_code,
	evm::{runtime::GAS_PRICE, CallTrace, CallTracer, CallType, GenericTransaction},
	exec::Key,
	limits,
	storage::DeletionQueueManager,
	test_utils::{builder::Contract, *},
	tests::{
		builder, initialize_block,
		test_utils::{self, get_contract, get_contract_checked},
		Balances, CodeHashLockupDepositPercent, Contracts, DepositPerByte, DepositPerItem,
		RuntimeCall, RuntimeEvent, RuntimeOrigin, System, DEPOSIT_PER_BYTE,
	},
	tracing::trace,
	weights::WeightInfo,
	AccountId32Mapper, AccountInfo, AccountInfoOf, BalanceOf, BalanceWithDust, BumpNonce, Code,
	CodeInfoOf, Config, ContractInfo, DeletionQueueCounter, DepositLimit, Error, EthTransactError,
	HoldReason, Origin, Pallet, PristineCode, H160,
};
use assert_matches::assert_matches;
use codec::Encode;
use frame_support::{
	assert_err, assert_err_ignore_postinfo, assert_noop, assert_ok, derive_impl,
	pallet_prelude::EnsureOrigin,
	parameter_types,
	storage::child,
	traits::{
		fungible::{BalancedHold, Inspect, Mutate, MutateHold},
		tokens::Preservation,
		ConstU32, ConstU64, FindAuthor, OnIdle, OnInitialize, StorageVersion,
	},
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, FixedFee, IdentityFee, Weight, WeightMeter},
};
use frame_system::{EventRecord, Phase};
use pallet_revive_fixtures::compile_module;
use pallet_revive_uapi::{ReturnErrorCode as RuntimeReturnCode, ReturnFlags};
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use pretty_assertions::{assert_eq, assert_ne};
use sp_core::{Get, U256};
use sp_io::hashing::blake2_256;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	testing::H256,
	traits::{BlakeTwo256, Convert, IdentityLookup, One, Zero},
	AccountId32, BuildStorage, DispatchError, Perbill, TokenError,
};

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
