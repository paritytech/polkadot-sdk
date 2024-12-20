// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract PiggyBank {

    uint private balance;
    address public owner;

    constructor() {
        owner = msg.sender;
        balance = 0;
    }

    function deposit() public payable returns (uint) {
        balance += msg.value;
        return balance;
    }

    function getDeposit() public view returns (uint) {
        return balance;
    }

    function withdraw(uint withdrawAmount) public returns (uint remainingBal) {
        require(msg.sender == owner);
        balance -= withdrawAmount;
        (bool success, ) = payable(msg.sender).call{value: withdrawAmount}("");
        require(success, "Transfer failed");

        return balance;
    }
}

