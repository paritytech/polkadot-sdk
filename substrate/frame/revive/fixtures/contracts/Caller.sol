// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// Contract that always reverts in constructor
contract ChildRevert {
    constructor() {
        revert("ChildRevert: revert in constructor");
    }
}

contract Caller {
    function normal(address _callee, uint64 _value, bytes memory _data, uint64 _gas)
        external
        returns (bool success, bytes memory output)
    {
        (success, output) = _callee.call{value: _value, gas: _gas}(_data);
    }

    function delegate(address _callee, bytes memory _data, uint64 _gas)
        external
        returns (bool success, bytes memory output)
    {
        (success, output) = _callee.delegatecall{gas: _gas}(_data);
    }

    function staticCall(
        // Don't rename to `static` (it's a Rust keyword).
        address _callee,
        bytes memory _data,
        uint64 _gas
    ) external view returns (bool success, bytes memory output) {
        (success, output) = _callee.staticcall{gas: _gas}(_data);
    }

    function create(bytes memory initcode) external payable returns (address addr) {
        assembly {
            // CREATE with no value
            addr := create(0, add(initcode, 0x20), mload(initcode))
            if iszero(addr) {
                // bubble failure
                let returnDataSize := returndatasize()
                returndatacopy(0, 0, returnDataSize)
                revert(0, returnDataSize)
            }
        }
    }

    function createRevert() external returns (address addr) {
        try new ChildRevert() returns (ChildRevert c) {
            addr = address(c);
        } catch (bytes memory reason) {
            revert(string(reason));
        }
    }

    function create2(bytes memory initcode, bytes32 salt) external payable returns (address addr) {
        assembly {
            // CREATE2 with no value
            addr := create2(0, add(initcode, 0x20), mload(initcode), salt)
            if iszero(addr) {
                // bubble failure
                let returnDataSize := returndatasize()
                returndatacopy(0, 0, returnDataSize)
                revert(0, returnDataSize)
            }
        }
    }
}
