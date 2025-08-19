
use revm::bytecode::opcode::*;

pub fn make_evm_bytecode_from_runtime_code(runtime_code: &Vec<u8>) -> Vec<u8> {
    let runtime_code_len = runtime_code.len();
    assert!(runtime_code_len < 256);
    let mut init_code: Vec<u8> = vec![
        vec![PUSH1, 0x80_u8],
        vec![PUSH1, 0x40_u8],
        vec![MSTORE],
        vec![PUSH1, 0x40_u8],
        vec![MLOAD],
        vec![PUSH1, runtime_code_len as u8],
        vec![PUSH1, 0x13_u8],
        vec![DUP3],
        vec![CODECOPY],
        vec![PUSH1, runtime_code_len as u8],
        vec![SWAP1],
        vec![RETURN],
        vec![INVALID],
    ]
    .into_iter()
    .flatten()
    .collect();
    init_code.extend(runtime_code);
    init_code
}