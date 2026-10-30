// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;
contract Counter {
    uint64 public number;

    constructor() {
        number = 3;
    }

    function setNumber(uint64 newNumber) public returns (uint64) {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}

contract NestedCounter {
    Counter public counter;
    uint64 public number;


    constructor() {
        counter = new Counter();
        counter.setNumber(10);
        number = 7;
    }

    function nestedNumber() public returns (uint64) {
        uint64 currentNumber = counter.setNumber(number);
        number++;
        return currentNumber;
    }
}
