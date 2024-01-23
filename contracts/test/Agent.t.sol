// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import {Agent} from "../src/Agent.sol";

contract Executor {
    error Failure();

    function foo() external pure returns (bool) {
        return true;
    }

    function fail() external pure {
        revert Failure();
    }
}

contract AgentTest is Test {
    bytes32 public constant AGENT_ID = keccak256("1000");
    Agent public agent;
    address public executor;

    function setUp() public {
        agent = new Agent(AGENT_ID);
        executor = address(new Executor());
    }

    function testInvoke() public {
        (bool success, bytes memory result) = agent.invoke(executor, abi.encodeCall(Executor.foo, ()));
        assertEq(success, true);
        assertEq(result, abi.encode(true));
    }

    function testInvokeUnauthorized() public {
        address user = makeAddr("user");

        vm.expectRevert(Agent.Unauthorized.selector);

        hoax(user);
        agent.invoke(executor, abi.encodeCall(Executor.foo, ()));
    }

    function testInvokeFail() public {
        (bool success, bytes memory result) = agent.invoke(executor, abi.encodeCall(Executor.fail, ()));
        assertEq(success, false);
        assertEq(result, bytes.concat(Executor.Failure.selector));
    }
}
