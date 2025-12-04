// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Terminate } from "./Terminate.sol";
import { TerminateDelegator } from "./TerminateDelegator.sol";

contract TerminateCaller {
    Terminate inner;
    TerminateDelegator innerCaller;
    receive() external payable {}
    
    constructor() payable {}

    function createAndTerminateTwice(uint value, uint8 method1, uint8 method2, address beneficiary) external returns (address) {
        inner = new Terminate{value: value}(true, method1, beneficiary);
        inner.terminate(method1, beneficiary);
        inner.terminate(method2, beneficiary);
        return address(inner);
    }

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
        revert("Deliberate revert");
    }

    function delegateCallTerminate(uint value, uint8 method, address beneficiary) external returns (address, address) {
        inner = new Terminate(true, method, beneficiary);
        innerCaller = new TerminateDelegator{value: value}();
        bytes memory data = abi.encodeWithSelector(innerCaller.delegateCallTerminate.selector, address(inner), method, beneficiary);
        (bool success, ) = address(innerCaller).call(data);
        require(success, "delegatecall terminate reverted");
        return (address(innerCaller), address(inner));
    }

    function callAfterTerminate(uint value, uint8 method) external returns (address, uint) {
        inner = new Terminate(true, method, payable(address(this)));
        inner.terminate(0, payable(address(this)));
        bytes memory data = abi.encodeWithSelector(inner.echo.selector, value);
        (bool success, bytes memory returnData) = address(inner).call(data);
        require(success, "call after terminate reverted");
        return (address(inner), returnData.length == 32 ? abi.decode(returnData, (uint)) : 0);
    }
}