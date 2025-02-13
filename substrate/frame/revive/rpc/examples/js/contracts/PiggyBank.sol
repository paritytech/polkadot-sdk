// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract PiggyBank {

    uint256 private balance;
    address public owner;

    constructor() public {
        owner = msg.sender;
        balance = 0;
    }

    function deposit() public payable returns (uint256) {
        balance += msg.value;
        return balance;
    }

    function getDeposit() public view returns (uint256) {
        return balance;
    }

    function withdraw(uint256 withdrawAmount) public returns (uint256 remainingBal) {
		require(msg.sender == owner, "You are not the owner");
        balance -= withdrawAmount;
        (bool success, ) = payable(msg.sender).call{value: withdrawAmount}("");
        require(success, "Transfer failed");

        return balance;
    }
}

