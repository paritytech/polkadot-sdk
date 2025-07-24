// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract TestSha3 {
    function test(string memory _pre) external payable returns (bytes32) {
        return keccak256(bytes(_pre));
    }
}
