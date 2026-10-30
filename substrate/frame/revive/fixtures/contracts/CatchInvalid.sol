// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

import { Callee } from "./Callee.sol";

contract InvalidConstructor {
    constructor() {
        assembly {
            invalid()
        }
    }
}

// Calls `invalid()` on the given callee, or creates a contract whose constructor hits INVALID, and
// records in storage that the catch block ran.
contract CatchInvalid {
    bool public caught;

    function tryInvalid(address callee) external {
        try Callee(callee).invalid() {} catch {
            caught = true;
        }
    }

    function tryCreateInvalid() external {
        try new InvalidConstructor() {} catch {
            caught = true;
        }
    }
}
