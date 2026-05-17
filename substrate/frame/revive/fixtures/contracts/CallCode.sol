// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract CodeCallee {
    // Read under CALLCODE actually hits the caller's slot 0.
    uint256 some_slot;

    // Returns (caller(), msg.value) for the caller to assert.
    fallback() external payable {
        // Caller seeds slot 0 before delegating.
        require(some_slot != 0, "slot 0 must be set via caller's storage");
        assembly {
            mstore(0, caller())
            mstore(0x20, callvalue())
            return(0, 64)
        }
    }
}

contract CodeCaller {
    // Preset so the borrowed code observes a non-zero value.
    uint256 some_slot = 1111111111;

    // Payable to allow funding at deploy for value-bearing tests.
    constructor() payable {}

    // CALLCODE with value=0; asserts msg.sender==self and msg.value==0.
    function doCallCode(address target) external payable returns (bool) {
        assembly {
            if iszero(callcode(gas(), target, 0, 0, 0, 0, 0)) {
                revert(0, 0)
            }
            returndatacopy(0, 0, returndatasize())
            if iszero(eq(address(), mload(0))) {
                revert(0, 0)
            }
            if iszero(eq(0, mload(0x20))) {
                revert(0, 0)
            }
            mstore(0, 1)
            return(0, 32)
        }
    }

    // CALLCODE with non-zero value; balance must not move (self -> self).
    function doCallCodeWithValue(address target, uint256 value) external payable returns (bool) {
        assembly {
            if iszero(callcode(gas(), target, value, 0, 0, 0, 0)) {
                revert(0, 0)
            }
            returndatacopy(0, 0, returndatasize())
            if iszero(eq(address(), mload(0))) {
                revert(0, 0)
            }
            if iszero(eq(value, mload(0x20))) {
                revert(0, 0)
            }
            mstore(0, 1)
            return(0, 32)
        }
    }

    // Returns the raw success bit; lets tests observe failures without reverting.
    function tryCallCodeWithValue(address target, uint256 value) external payable returns (bool ok) {
        assembly {
            ok := callcode(gas(), target, value, 0, 0, 0, 0)
        }
    }
}
