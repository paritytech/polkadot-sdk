// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Callee {
    function echo(uint256 value) external pure returns (uint256) {
        return value;
    }
}
