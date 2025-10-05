// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Callee {
    uint64 public stored;

    function echo(uint64 _data) external pure returns (uint64 data) {
        data = _data;
    }

    function whoSender() external view returns (address) {
        return msg.sender;
    }

    function store(uint64 _data) external {
        stored = _data;
    }

    function revert() public pure returns (uint64) {
        require(false, "This is a revert");
        return 42; // never reached
    }

    function invalid() public pure {
        assembly {
            invalid()
        }
    }

    function stop() public pure {
        assembly {
            stop()
        }
    }
}
