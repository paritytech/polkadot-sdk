// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;

/// Emits a log and then either returns or reverts, from its own frame or from a sub-call, so a
/// test can watch where the log ends up.
contract Emitter {
    event Emitted(uint64 value);
    event Doomed(uint64 value);

    function emitValue(uint64 value) public {
        emit Emitted(value);
    }

    function emitThenRevert(uint64 value) public {
        emit Doomed(value);
        revert("reverted after emitting");
    }

    /// Emits `kept`, then lets a sub-call emit `dropped` and revert, swallowing the revert.
    function emitAndCallReverting(uint64 kept, uint64 dropped) public {
        emit Emitted(kept);
        try this.emitThenRevert(dropped) {} catch {}
    }
}
