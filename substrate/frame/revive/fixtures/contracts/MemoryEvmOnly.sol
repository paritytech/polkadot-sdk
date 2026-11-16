// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

// Resolc doesn't support CODECOPY in runtime code, so this contract is compiled with Solc only.
contract MemoryEvmOnly {
    // Each copy writes into memory expanded to 1 KiB and returns the same 64-byte range.
    // Calls that differ only in len execute the same op-codes.
    function calldatacopyWithoutGrowth(uint256 len) public pure {
        assembly {
            mstore(0x3e0, 0)
            calldatacopy(0x100, 0, len)
            return(0x100, 64)
        }
    }

    function codecopyWithoutGrowth(uint256 len) public pure {
        assembly {
            mstore(0x3e0, 0)
            codecopy(0x100, 0, len)
            return(0x100, 64)
        }
    }

    function returndatacopyWithoutGrowth(uint256 len) public {
        this.returnData();
        assembly {
            mstore(0x3e0, 0)
            returndatacopy(0x100, 0, len)
            return(0x100, 64)
        }
    }

    function returnData() public pure {
        assembly {
            mstore(0, 0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a)
            mstore(32, 0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a)
            return(0, 64)
        }
    }
}
