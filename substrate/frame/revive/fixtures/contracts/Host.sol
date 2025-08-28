// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Host {
    function balance(address account) public view returns (uint256) {
        return account.balance;
    }

    function extcodesizeOp(address account) public view returns (uint256) {
        uint256 size;
        assembly {
            size := extcodesize(account)
        }
        return size;
    }

    function extcodehashOp(address account) public view returns (bytes32) {
        bytes32 hash;
        assembly {
            hash := extcodehash(account)
        }
        return hash;
    }

    function blockhashOp(uint256 blockNumber) public view returns (bytes32) {
        bytes32 hash;
        assembly {
            hash := blockhash(blockNumber)
        }
        return hash;
    }

    function sloadOp(uint256 slot) public view returns (uint256) {
        uint256 value;
        assembly {
            value := sload(slot)
        }
        return value;
    }

    function sstoreOp(uint256 slot, uint256 value) public {
        assembly {
            sstore(slot, value)
        }
    }

    function log0Op(bytes32 data) public {
        assembly {
            log0(data, 0x20)
        }
    }

    function log1Op(bytes32 data, bytes32 topic1) public {
        assembly {
            log1(data, 0x20, topic1)
        }
    }

    function log2Op(bytes32 data, bytes32 topic1, bytes32 topic2) public {
        assembly {
            log2(data, 0x20, topic1, topic2)
        }
    }

    function log3Op(
        bytes32 data,
        bytes32 topic1,
        bytes32 topic2,
        bytes32 topic3
    ) public {
        assembly {
            log3(data, 0x20, topic1, topic2, topic3)
        }
    }

    function log4Op(
        bytes32 data,
        bytes32 topic1,
        bytes32 topic2,
        bytes32 topic3,
        bytes32 topic4
    ) public {
        assembly {
            log4(data, 0x20, topic1, topic2, topic3, topic4)
        }
    }

    function selfbalance() public view returns (uint256) {
        return address(this).balance;
    }
}
