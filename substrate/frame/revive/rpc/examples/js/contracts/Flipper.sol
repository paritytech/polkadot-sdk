// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Flipper - Stores and toggles a boolean value
contract Flipper {
    bool public value;

    function flip() external {
		value = !value;
    }

    function getValue() external view returns (bool) {
        return value;
    }
}

// FlipperCaller - Interacts with the Flipper contract
contract FlipperCaller {
    // Address of the Flipper contract
    address public flipperAddress;

    // Constructor to initialize Flipper's address
    constructor(address _flipperAddress) public {
        flipperAddress = _flipperAddress;
    }

    function callFlip() external {
        Flipper(flipperAddress).flip();
    }

    function callGetValue() external view returns (bool) {
        return Flipper(flipperAddress).getValue();
    }
}

