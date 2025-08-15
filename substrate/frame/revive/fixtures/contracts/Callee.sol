// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Callee {
    uint public stored;

    function echo(uint _data) external pure returns (uint data) {
        data = _data;
    }

    function whoSender() external view returns (address) {
        return msg.sender;
    }

    function store(uint _data) external {
        stored = _data;
    }
}
