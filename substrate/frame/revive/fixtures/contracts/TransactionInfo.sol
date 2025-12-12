// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TransactionInfo {
    function origin() public view returns (address) {
        return tx.origin;
    }

    function gasprice() public view returns (uint64) {
        return uint64(tx.gasprice);
    }

    function blobhash(uint64 index) public view returns (bytes32) {
        return blobhash(index);
    }
}
