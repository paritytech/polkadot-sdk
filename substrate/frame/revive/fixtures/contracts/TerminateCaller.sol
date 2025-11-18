// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import { Terminate } from "./Terminate.sol";
import { TerminateDelegator } from "./TerminateDelegator.sol";

contract TerminateCaller {
    Terminate inner;
    TerminateDelegator innerCaller;
    receive() external payable {}
    
    constructor() payable {}

    function sendFundsAfterTerminateAndCreate(uint value, uint8 method, address beneficiary) external returns (address) {
        inner = new Terminate(true, method, beneficiary);
        inner.terminate(method, beneficiary);
        (bool success, ) = address(inner).call{value: value}("");
        require(success, "terminate reverted");
        return address(inner);
    }

    function sendFundsAfterTerminate(address payable terminate_addr, uint value, uint8 method, address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, method, beneficiary));
        (bool success, ) = terminate_addr.call{value: value}("");
        require(success, "terminate reverted");
    }

    function revertAfterTerminate(address terminate_addr, uint8 method, address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, method, beneficiary));
        _revert();
    }

    function delegateCallTerminate(uint value, uint8 method, address beneficiary) external returns (address, address) {
        inner = new Terminate(true, method, beneficiary);
        innerCaller = new TerminateDelegator{value: value}();
        bytes memory data = abi.encodeWithSelector(innerCaller.delegateCallTerminate.selector, address(inner), method, beneficiary);
        (bool success, ) = address(innerCaller).call(data);
        require(success, "delegatecall terminate reverted");
        return (address(innerCaller), address(inner));
    }

    function _revert() private {
        revert("Deliberate revert");
    }
}