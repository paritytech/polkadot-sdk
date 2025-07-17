// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Balance Contract
/// @notice Tests the balance API. It checks the current balance of the contract.
contract Balance {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main call function that tests balance checking
    function call() public view {
        uint256 balance = address(this).balance;
        require(balance == 0, "Balance should be 0");
    }
    
    /// @notice Allow the contract to receive Ether
    receive() external payable {}
}