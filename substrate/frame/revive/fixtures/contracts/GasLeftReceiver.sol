// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract GasLeftReceiver {
    /// Returns the gas left. It is payable, so a call that sends value does not revert.
    function gasLeft() external payable returns (uint64) {
        return uint64(gasleft());
    }
}
