// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallReturnCode {
    uint256 number;

    function store(uint256 num) public {
        number = num;
    }

    function retrieve() public view returns (uint256) {
        return number;
    }
}