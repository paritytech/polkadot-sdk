// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BalanceOf {
    constructor() {
        // Empty constructor
    }
    
    function call(address account) external view {
        uint256 balance = account.balance;
        require(balance != 0, "Balance should not be zero");
    }
}