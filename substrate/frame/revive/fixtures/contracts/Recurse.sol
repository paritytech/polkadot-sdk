// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title Recurse
 * @dev Calls itself `callsLeft` times, then calls `finalTarget` once from the innermost frame.
 * A zero `finalTarget` skips that final call. Both results are ignored, so a call that fails
 * leaves the frame that made it running.
 */
contract Recurse {
    function recurse(uint32 callsLeft, address finalTarget) external {
        if (callsLeft > 0) {
            address(this).call(
                abi.encodeWithSignature(
                    "recurse(uint32,address)",
                    callsLeft - 1,
                    finalTarget
                )
            );
        } else if (finalTarget != address(0)) {
            finalTarget.call("");
        }
    }
}
