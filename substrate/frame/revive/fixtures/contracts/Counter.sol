// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;
contract Counter {
    uint256 public number;

    constructor() {
        number = 3;
    }

    function setNumber(uint256 newNumber) public returns (uint256) {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}

contract NestedCounter {
    Counter public counter;
    uint256 public number;


    constructor() {
        counter = new Counter();
        counter.setNumber(10);
        number = 7;
    }

    function nestedNumber() public returns (uint256) {
        uint256 currentNumber = counter.setNumber(number);
        number++;
        return currentNumber;
    }
}
