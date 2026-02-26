// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Treasury {
    // Mapping of spends to their approval and maturity blocks
    mapping (uint256 => Spend) public spends;
    // Next spend to be paid out
    uint256 public nextPayout;
    // Treasury balance
    uint256 public balance;
    // Daily acquisition of assets
    uint256 public dailyAcquisition;

    // Struct to represent a spend
    struct Spend {
        uint256 amount;
        uint256 approvedBlock;
        uint256 maturedBlock;
    }

    // Function to approve a spend
    function approveSpend(uint256 _amount, uint256 _maturedBlock) external {
        // Check if the spend can be approved
        require(_amount > 0, "Amount must be greater than 0");
        require(_maturedBlock > block.number, "Matured block must be in the future");
        // Add the spend to the mapping
        spends[nextPayout] = Spend(_amount, block.number, _maturedBlock);
        // Increment the next payout
        nextPayout++;
    }

    // Function to pay out a spend
    function payout() external {
        // Check if there is a next payout
        require(nextPayout > 0, "No spends to payout");
        // Get the next spend
        Spend storage spend = spends[nextPayout - 1];
        // Check if the spend has matured
        require(spend.maturedBlock <= block.number, "Spend has not matured");
        // Check if the treasury has sufficient balance
        require(balance >= spend.amount, "Insufficient treasury balance");
        // Pay out the spend
        balance -= spend.amount;
        // Remove the spend from the mapping
        delete spends[nextPayout - 1];
        // Decrement the next payout
        nextPayout--;
    }

    // Function to update the treasury balance
    function updateBalance(uint256 _balance) external {
        balance = _balance;
    }

    // Function to update the daily acquisition
    function updateDailyAcquisition(uint256 _dailyAcquisition) external {
        dailyAcquisition = _dailyAcquisition;
    }
}