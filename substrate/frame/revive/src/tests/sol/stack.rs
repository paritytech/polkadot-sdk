use crate::{
	test_utils::{builder::Contract, ALICE, },
	tests::{builder, ExtBuilder, Test, 
    revm_test_utils::make_evm_bytecode_from_runtime_code},
	Code, Config,
};
use alloy_core::primitives::U256;
use frame_support::traits::fungible::Mutate;
use pretty_assertions::assert_eq;
use pallet_revive_uapi::ReturnFlags;

use revm::bytecode::opcode::*;

#[test]
fn push_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .gas_limit(1_000_000_000.into())
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            result.flags != ReturnFlags::REVERT,
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return 0xfefefefe"
        );
    });
}

#[test]
fn pop_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH1, 0xaa],
        vec![POP],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .gas_limit(1_000_000_000.into())
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            result.flags != ReturnFlags::REVERT,
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return 0xfefefefe"
        );
    });
}

#[test]
fn dup_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
        vec![DUP2],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .gas_limit(1_000_000_000.into())
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            result.flags != ReturnFlags::REVERT,
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return 0xfefefefe"
        );
    });
}


#[test]
fn swap_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
        vec![SWAP1],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .gas_limit(1_000_000_000.into())
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            result.flags != ReturnFlags::REVERT,
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return 0xfefefefe"
        );
    });
}