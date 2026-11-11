// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title SendBackOrForward
 * @dev Accepts value and sends `amount` back to its caller, or on to another account.
 */
contract SendBackOrForward {
    function sendBack(uint64 amount) external payable {
        (bool success, ) = msg.sender.call{value: amount}("");
        require(success, "sending back failed");
    }

    function sendTo(address to, uint64 amount) external payable {
        (bool success, ) = to.call{value: amount}("");
        require(success, "sending failed");
    }
}
