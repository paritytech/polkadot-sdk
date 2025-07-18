// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BalanceOf {
    constructor() {
        // Empty constructor
    }
    
    fallback() external {
        // Extract address from call data (first 20 bytes)
        require(msg.data.length >= 20, "Not enough data");
        
        address account;
        assembly {
            account := shr(96, calldataload(0))
        }
        
        uint256 balance = account.balance;
        require(balance != 0, "Balance should not be zero");
    }
}