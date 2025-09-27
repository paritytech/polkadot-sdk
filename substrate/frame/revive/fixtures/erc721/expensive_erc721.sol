// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ERC721Flatt.sol";

contract GasHeavyNFT is ERC721 {
    constructor() ERC721("GasHeavyNFT", "GHNFT") {
        _mint(msg.sender, 0);
    }

    function transferFrom(
        address from,
        address to,
        uint256 tokenId
    ) public override {
        super.transferFrom(from, to, tokenId);

        // Simula un consumo pesante di gas
        for (uint256 i = 0; i < 1000000; i++) {
            keccak256(abi.encode(i, tokenId));
        }
    }
}
