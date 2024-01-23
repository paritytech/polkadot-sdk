// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2023 Axelar Network
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

pragma solidity 0.8.23;

library Address {
    // Checks whether `account` is a contract
    function isContract(address account) internal view returns (bool) {
        // https://eips.ethereum.org/EIPS/eip-1052
        // keccak256('') == 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        return account.codehash != bytes32(0)
            && account.codehash != 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470;
    }
}
