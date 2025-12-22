// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract FakeNFT {
    mapping(uint256 => address) private _owners;

    constructor() {
        _owners[0] = msg.sender;
    }

    function transferFrom(
        address from,
        address to,
        uint256 tokenId
    ) public returns (uint256) {
        require(_owners[tokenId] == from, "Not owner");
        _owners[tokenId] = to;

        // returns a dummy value
        return 1234;
    }

    function ownerOf(uint256 tokenId) public view returns (address) {
        return _owners[tokenId];
    }
}
