// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

contract Flipper {
    bool public coin;

    fallback() external {
        coin = !coin;
    }
}
