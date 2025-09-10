// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract BlockInfo {
    function blockNumber() public view returns (uint) {
        return block.number;
    }

    function coinbase() public view returns (address) {
        return block.coinbase;
    }

    function timestamp() public view returns (uint) {
        return block.timestamp;
    }

    function difficulty() public view returns (uint) {
        return block.difficulty;
    }

    function gaslimit() public view returns (uint) {
        return block.gaslimit;
    }

    function chainid() public view returns (uint) {
        return block.chainid;
    }

    function basefee() public view returns (uint) {
        return block.basefee;
    }
}
