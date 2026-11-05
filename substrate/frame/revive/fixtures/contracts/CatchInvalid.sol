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

contract PayableInvalid {
    function invalid() external payable {
        assembly {
            invalid()
        }
    }
}

// Makes calls and creates contracts that hit INVALID, and reports whether the catch block ran.
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

    function tryPayableInvalid(address callee, uint64 value) external returns (bool) {
        try PayableInvalid(callee).invalid{value: value}() {} catch {
            return true;
        }
        return false;
    }
}
