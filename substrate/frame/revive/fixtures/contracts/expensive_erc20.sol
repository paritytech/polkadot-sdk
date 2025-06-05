// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MyToken is ERC20 {
    constructor(uint256 total) ERC20("TestToken1", "TT1") {
        // We mint `total` tokens to the creator of this contract, as
        // a sort of genesis.
        _mint(msg.sender, total);
    }

    function transfer(address to, uint256 value) public override returns (bool) {
        address owner = msg.sender;
        _transfer(owner, to, value);
        for (uint256 i = 0; i < 1000000; i++) {
            keccak256(abi.encode(i));
        }
        return true;
    }
}
