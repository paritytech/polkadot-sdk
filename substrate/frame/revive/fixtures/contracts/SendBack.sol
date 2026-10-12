// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title SendBack
 * @dev Accepts value and sends `amount` back to its caller.
 */
contract SendBack {
    function sendBack(uint64 amount) external payable {
        (bool success, ) = msg.sender.call{value: amount}("");
        require(success, "sending back failed");
    }
}
