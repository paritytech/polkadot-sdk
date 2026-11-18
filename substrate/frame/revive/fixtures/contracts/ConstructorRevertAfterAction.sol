// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import { Dummy } from "./Dummy.sol";

/**
 * @title ConstructorRevertAfterAction
 * @dev Its constructor runs the given action and reverts
 */
contract ConstructorRevertAfterAction {
    enum Action { ReadOwnBalance, WriteStorage, Call, Create }

    error Balance(uint256 balance);

    uint256 value;

    constructor(Action action) {
        if (action == Action.ReadOwnBalance) {
            uint256 ownBalance;
            // solc compiles `address(this).balance` to SELFBALANCE, so BALANCE needs assembly.
            assembly { ownBalance := balance(address()) }
            // Reverting with the balance keeps the compiler from dropping the read.
            revert Balance(ownBalance);
        } else if (action == Action.WriteStorage) {
            value = 1;
        } else if (action == Action.Call) {
            msg.sender.call("");
        } else {
            new Dummy();
        }
        revert();
    }
}
