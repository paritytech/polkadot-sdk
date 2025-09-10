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

    function revert() public pure returns (uint256) {
        require(false, "This is a revert");
        return 42; // never reached
    }

    function invalid() public pure returns (uint256 result) {
        assembly {
            invalid() // 0xFE opcode
        }
    }

    function stop() public pure returns (uint256 result) {
        assembly {
            stop()
        }
    }
}
