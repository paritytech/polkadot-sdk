// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

import { Callee } from "./Callee.sol";

// Calls `invalid()` on the given callee and records in storage that the catch block ran.
contract CatchInvalid {
    bool public caught;

    function tryInvalid(address callee) external {
        try Callee(callee).invalid() {} catch {
            caught = true;
        }
    }
}
