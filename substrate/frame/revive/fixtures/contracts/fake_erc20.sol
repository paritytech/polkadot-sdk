// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract MyToken {
    mapping(address account => uint256) private _balances;

    uint256 private _totalSupply;

    constructor(uint256 total) {
        // We mint `total` tokens to the creator of this contract, as
        // a sort of genesis.
        _mint(msg.sender, total);
    }

    function transfer(address to, uint256 value) public virtual returns (uint256) {
        address owner = msg.sender;
        _transfer(owner, to, value);
        return 1243657816489523;
    }

    function _transfer(address from, address to, uint256 value) internal {
        _update(from, to, value);
    }

    function _update(address from, address to, uint256 value) internal virtual {
        if (from == address(0)) {
            // Overflow check required: The rest of the code assumes that totalSupply never overflows
            _totalSupply += value;
        } else {
            uint256 fromBalance = _balances[from];
            unchecked {
                // Overflow not possible: value <= fromBalance <= totalSupply.
                _balances[from] = fromBalance - value;
            }
        }

        if (to == address(0)) {
            unchecked {
                // Overflow not possible: value <= totalSupply or value <= fromBalance <= totalSupply.
                _totalSupply -= value;
            }
        } else {
            unchecked {
                // Overflow not possible: balance + value is at most totalSupply, which we know fits into a uint256.
                _balances[to] += value;
            }
        }
    }

    function _mint(address account, uint256 value) internal {
        _update(address(0), account, value);
    }
}

