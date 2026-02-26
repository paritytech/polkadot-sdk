// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "https://github.com/OpenZeppelin/openzeppelin-solidity/contracts/test_helpers/crowsale_helper.sol";

contract TreasuryTest {
    Treasury public treasury;

    function setUp() public {
        treasury = new Treasury();
    }

    function testApproveSpend() public {
        // Approve a spend
        treasury.approveSpend(100, block.number + 10);
        // Check if the spend was added to the mapping
        assert(treasury.spends(0).amount == 100);
    }

    function testPayout() public {
        // Approve a spend
        treasury.approveSpend(100, block.number);
        // Update the treasury balance
        treasury.updateBalance(100);
        // Pay out the spend
        treasury.payout();
        // Check if the spend was removed from the mapping
        assert(treasury.spends(0).amount == 0);
    }

    function testUpdateBalance() public {
        // Update the treasury balance
        treasury.updateBalance(100);
        // Check if the balance was updated
        assert(treasury.balance() == 100);
    }
}