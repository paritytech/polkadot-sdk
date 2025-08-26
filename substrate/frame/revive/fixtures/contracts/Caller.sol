// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Caller {
    function normal(
        address _callee,
        uint _value,
        bytes memory _data,
        uint _gas
    ) external returns (bool success, bytes memory output) {
        (success, output) = _callee.call{value: _value, gas: _gas}(_data);
    }

    function delegate(
        address _callee,
        bytes memory _data,
        uint _gas
    ) external returns (bool success, bytes memory output) {
        (success, output) = _callee.delegatecall{gas: _gas}(_data);
    }

    function staticCall(
        // Don't rename to `static` (it's a Rust keyword).
        address _callee,
        bytes memory _data,
        uint _gas
    ) external returns (bool success, bytes memory output) {
        (success, output) = _callee.staticcall{gas: _gas}(_data);
    }

    function create(bytes memory initcode) external returns (address addr) {
        assembly {
            // CREATE with no value
            addr := create(0, add(initcode, 0x20), mload(initcode))
            if iszero(addr) {
                // bubble failure
                revert(0, 0)
            }
        }
    }

    function create2(
        bytes memory initcode,
        bytes32 salt
    ) external returns (address addr) {
        assembly {
            // CREATE2 with no value
            addr := create2(0, add(initcode, 0x20), mload(initcode), salt)
            if iszero(addr) {
                // bubble failure
                revert(0, 0)
            }
        }
    }
}
