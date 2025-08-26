// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Host {
    function balance(address account) public view returns (uint256) {
        return account.balance;
    }

    function extcodesize(address account) public view returns (uint256) {
        uint256 size;
        assembly {
            size := extcodesize(account)
        }
        return size;
    }

    function extcodecopy(
        address /* account */,
        uint256 /* destOffset */,
        uint256 /* offset */,
        uint256 size
    ) public pure returns (bytes memory) {
        bytes memory code = new bytes(size);
        return code;
    }

    function extcodehash(address account) public view returns (bytes32) {
        bytes32 hash;
        assembly {
            hash := extcodehash(account)
        }
        return hash;
    }

    function blockhash(uint256 blockNumber) public view returns (bytes32) {
        return blockhash(blockNumber);
    }

    function sload(uint256 slot) public view returns (uint256) {
        uint256 value;
        assembly {
            value := sload(slot)
        }
        return value;
    }

    function sstore(uint256 slot, uint256 value) public returns (uint256) {
        assembly {
            sstore(slot, value)
        }
        return value;
    }

    function tload(uint256 slot) public view returns (uint256) {
        uint256 value;
        assembly {
            value := tload(slot)
        }
        return value;
    }

    function tstore(uint256 slot, uint256 value) public returns (uint256) {
        assembly {
            tstore(slot, value)
        }
        return value;
    }

    function log0(bytes32 data) public {
        assembly {
            log0(data, 0x20)
        }
    }

    function log1(bytes32 data, bytes32 topic1) public {
        assembly {
            log1(data, 0x20, topic1)
        }
    }

    function log2(bytes32 data, bytes32 topic1, bytes32 topic2) public {
        assembly {
            log2(data, 0x20, topic1, topic2)
        }
    }

    function log3(
        bytes32 data,
        bytes32 topic1,
        bytes32 topic2,
        bytes32 topic3
    ) public {
        assembly {
            log3(data, 0x20, topic1, topic2, topic3)
        }
    }

    function log4(
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

    function selfdestruct(address payable recipient) public {
        selfdestruct(recipient);
    }

    function selfbalance() public view returns (uint256) {
        return address(this).balance;
    }
}
