// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MyToken is ERC20 {
    constructor(uint256 total) ERC20("TestToken1", "TT1") {
        // We mint `total` tokens to the creator of this contract, as
        // a sort of genesis.
        _mint(msg.sender, total);
    }

    function mint(uint256 amount) public {
        _mint(msg.sender, amount);
    }

    function burn(uint256 amount) public {
        _burn(msg.sender, amount);
    }
}
