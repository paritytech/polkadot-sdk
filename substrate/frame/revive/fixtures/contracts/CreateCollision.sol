// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract CreateCollisionFactory {
    function deploy(bytes32 salt) external returns (address) {
        try new CreateCollisionChild{salt: salt}() returns (CreateCollisionChild child) {
            return address(child);
        } catch {
            return address(0);
        }
    }

    function deployReverting(bytes32 salt) external returns (address) {
        try new CreateCollisionRevertingChild{salt: salt}() returns (
            CreateCollisionRevertingChild child
        ) {
            return address(child);
        } catch {
            return address(0);
        }
    }
}

contract CreateCollisionChild {}

contract CreateCollisionRevertingChild {
    constructor() {
        revert();
    }
}
