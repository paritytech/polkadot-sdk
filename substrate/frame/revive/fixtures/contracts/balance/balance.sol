// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Balance Contract
/// @notice Tests the balance API. It checks the current balance of the contract.
contract Balance {
    bool private called = false;
    
    fallback() external payable {
        if (!called) {
            // First call: should succeed
            called = true;
            return;
        } else {
            // Second call: should fail
            require(false, "Second call should fail");
        }
    }
}