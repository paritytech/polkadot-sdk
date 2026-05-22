// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract NestedDeployer {
    function deployChild() external returns (address) {
        return address(new NestedChild());
    }
}

contract NestedChild {
    uint256 public state;

    constructor() {
        state = 42;
    }

    /// Self-terminate via the system precompile (`only_if_same_tx: false`), so the
    /// contract can be destroyed in a later tx than the one that created it.
    function destroyViaPrecompile(address beneficiary) external {
        bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
        (bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
        if (!success) {
            assembly {
                revert(add(returnData, 0x20), mload(returnData))
            }
        }
    }
}
