// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

contract System {
    function keccak256(string memory _pre) external payable returns (bytes32) {
        return keccak256(bytes(_pre));
    }
}
