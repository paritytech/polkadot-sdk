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

    function logOps() public {
        assembly {
            log0(0x01, 0x20)
            log1(0x02, 0x20, 0x11)
            log2(0x03, 0x20, 0x22, 0x33)
            log3(0x04, 0x20, 0x44, 0x55, 0x66)
            log4(0x05, 0x20, 0x77, 0x88, 0x99, 0xaa)
        }
    }

    function selfbalance() public view returns (uint256) {
        return address(this).balance;
    }
}
