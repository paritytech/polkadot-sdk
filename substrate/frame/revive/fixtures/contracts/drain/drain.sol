// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Drain {
    constructor() {
        // Empty constructor
    }
    
    function call() external {
        uint256 balance = address(this).balance;
        
        // Add a minimum balance amount to exceed current balance
        // In EVM, we'll use a small amount to simulate minimum balance
        uint256 minimumBalance = 1 wei;
        uint256 transferAmount = balance + minimumBalance;
        
        // Try to transfer more than available balance to address(0)
        // This should fail because we don't have enough balance
        (bool success, ) = address(0).call{value: transferAmount}("");
        
        // The transfer should fail
        require(!success, "Transfer should have failed");
    }
    
    // Allow contract to receive ether
    receive() external payable {}
}