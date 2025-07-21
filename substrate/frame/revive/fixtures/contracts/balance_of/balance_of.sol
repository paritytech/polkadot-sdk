// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BalanceOf {
    constructor() {
        // Empty constructor
    }
    
    fallback() external {
        // Extract address from call data (first 20 bytes)
        address account;
        assembly {
            if lt(calldatasize(), 20) {
                revert(0, 0)
            }
            account := shr(96, calldataload(0))
        }
        
        assembly {
            let bal := balance(account)
            if iszero(bal) {
                invalid()
            }
        }
    }
}