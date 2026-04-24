// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// Exercises the CALLCODE opcode (reachable only via YUL since solc v0.3.0).
//
// CALLCODE runs the target contract's code in the caller's storage context, with
// msg.sender set to the caller and msg.value taken from the stack argument.

contract CodeCallee {
    // Slot 0 — when executed under CALLCODE this reads the caller's slot 0.
    uint256 some_slot;

    // Returns the msg.sender it sees. When executed via CALLCODE from CodeCaller,
    // this should be CodeCaller's own address.
    fallback() external payable {
        // Running under CALLCODE the storage is the caller's. The caller sets
        // slot 0 to a non-zero value before delegating.
        require(some_slot != 0, "slot 0 must be set via caller's storage");
        // CALLCODE was invoked with value 0 on the stack.
        require(msg.value == 0, "expected msg.value == 0");
        assembly {
            mstore(0, caller())
            return(0, 32)
        }
    }
}

contract CodeCaller {
    // Slot 0 is set so the callee (running in our storage) observes it.
    uint256 some_slot = 1111111111;

    // Invokes CALLCODE against `target` with value=0 and empty calldata,
    // then verifies the returned address equals address(this).
    function doCallCode(address target) external payable returns (bool) {
        assembly {
            if iszero(callcode(gas(), target, 0, 0, 0, 0, 0)) {
                revert(0, 0)
            }
            returndatacopy(0, 0, returndatasize())
            if iszero(eq(address(), mload(0))) {
                revert(0, 0)
            }
            mstore(0, 1)
            return(0, 32)
        }
    }
}
