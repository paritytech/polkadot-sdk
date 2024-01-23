// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

/**
 * @title Initialization of gateway logic contracts
 */
interface IInitializable {
    function initialize(bytes calldata data) external;
}
