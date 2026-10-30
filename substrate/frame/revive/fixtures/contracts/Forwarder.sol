// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// Forwards a call with the requested gas and, like OpenZeppelin's `ERC2771Forwarder`, hits INVALID
// when the gas left afterwards shows that the callee may have received less than it requested.
contract Forwarder {
    function forward(address target, bytes calldata data, uint64 requestedGas)
        external
        returns (bool success)
    {
        (success, ) = target.call{gas: requestedGas}(data);
        if (gasleft() < requestedGas / 63) {
            assembly {
                invalid()
            }
        }
    }
}
