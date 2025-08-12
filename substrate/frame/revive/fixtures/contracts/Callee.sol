// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Callee {
    function echo(uint _data) external pure returns (uint data) {
        data = _data;
    }
}
