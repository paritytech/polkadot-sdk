// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ERC721Flatt.sol";

contract MyNFT is ERC721 {
    constructor() ERC721("TestNFT", "TNFT") {
        _mint(msg.sender, 0); 
    }
}
