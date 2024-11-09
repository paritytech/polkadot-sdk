// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract EventExample {
    event ExampleEvent(address indexed sender, uint256 value, string message);

    // Function to emit the event with hard-coded values
    function triggerEvent() public {
        uint256 value = 12345;
        string memory message = "Hello world";
        emit ExampleEvent(msg.sender, value, message);
    }
}

