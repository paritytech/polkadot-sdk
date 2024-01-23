// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

import {UD60x18} from "prb/math/src/UD60x18.sol";

library PricingStorage {
    struct Layout {
        /// @dev The ETH/DOT exchange rate
        UD60x18 exchangeRate;
        /// @dev The cost of delivering messages to BridgeHub in DOT
        uint128 deliveryCost;
    }

    bytes32 internal constant SLOT = keccak256("org.snowbridge.storage.pricing");

    function layout() internal pure returns (Layout storage $) {
        bytes32 slot = SLOT;
        assembly {
            $.slot := slot
        }
    }
}
