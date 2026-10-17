// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

// Resolc doesn't support CODECOPY in runtime code, so this contract is compiled with Solc only.
contract MemoryEvmOnly {
    // The loops below differ only in the zero-length copy they repeat `n` times.

    function zeroLengthCalldatacopies(uint256 n) public pure {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { calldatacopy(0, 0, 0) }
        }
    }

    function zeroLengthCodecopies(uint256 n) public pure {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { codecopy(0, 0, 0) }
        }
    }

    function zeroLengthReturndatacopies(uint256 n) public pure {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { returndatacopy(0, 0, 0) }
        }
    }

    function zeroLengthMcopies(uint256 n) public pure {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { mcopy(0, 0, 0) }
        }
    }
}
