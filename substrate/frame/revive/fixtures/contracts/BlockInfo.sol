// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract BlockInfo {
    function blockNumber() public view returns (uint64) {
        return uint64(block.number);
    }

    function coinbase() public view returns (address) {
        return block.coinbase;
    }

    function timestamp() public view returns (uint64) {
        return uint64(block.timestamp);
    }

    function difficulty() public view returns (uint64) {
        return uint64(block.difficulty);
    }

    function gaslimit() public view returns (uint64) {
        return uint64(block.gaslimit);
    }

    function chainid() public view returns (uint64) {
        return uint64(block.chainid);
    }

    function basefee() public view returns (uint64) {
        return uint64(block.basefee);
    }
}
