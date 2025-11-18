// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import { Terminate } from "./Terminate.sol";

contract TerminateCaller {
    Terminate inner;
    receive() external payable {}
    
    constructor() payable {}

    function sendFundsAfterTerminateAndCreate(uint value, uint8 method, address beneficiary) external returns (address addr) {
        // bytes memory init = abi.encodePacked(
        //     type(Terminate).creationCode,
        //     abi.encode(true, method, beneficiary) // constructor(bool skip, uint8 method, address beneficiary)
        // );
        // assembly {
        //     addr := create(0, add(init, 0x20), mload(init))
        // }
        inner = new Terminate(
            /* skip = */ true,
            method,
            beneficiary
        );
        inner.terminate(method, beneficiary);
        (bool success, ) = address(inner).call{value: value}("");
        require(success, "terminate reverted");
        return address(inner);
    }

    function sendFundsAfterTerminate(address payable terminate_addr, uint value, uint8 method,address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, method, beneficiary));
        (bool success, ) = terminate_addr.call{value: value}("");
        require(success, "terminate reverted");
    }

    function revertAfterTerminate(address terminate_addr, uint8 method, address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, method, beneficiary));
        _revert();
    }

    function _revert() private {
        revert("Deliberate revert");
    }
}