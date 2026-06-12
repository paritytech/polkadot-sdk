// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@revive/ISystem.sol";

/// Exercises the `System.originIsRoot` and `System.callerIsRoot` precompile methods
/// through various call shapes.
///
/// A single instance can play either role: the contract that ultimately invokes the
/// precompile, or a proxy that reaches another instance through a regular call or
/// delegate call.
contract OriginIsRoot {
	/// Directly invoke `originIsRoot()` on the System precompile.
	function originIsRoot() external view returns (bool) {
		return ISystem(SYSTEM_ADDR).originIsRoot();
	}

	/// Directly invoke `callerIsRoot()` on the System precompile.
	function callerIsRoot() external view returns (bool) {
		return ISystem(SYSTEM_ADDR).callerIsRoot();
	}

	/// Regular contract call into `target.originIsRoot()`.
	function callOriginIsRoot(address target) external view returns (bool) {
		return OriginIsRoot(target).originIsRoot();
	}

	/// Regular contract call into `target.callerIsRoot()`.
	function callCallerIsRoot(address target) external view returns (bool) {
		return OriginIsRoot(target).callerIsRoot();
	}

	/// Delegate-call into `impl.originIsRoot()`, the same shape as an upgradeable proxy
	/// dispatching into its implementation.
	function delegateOriginIsRoot(address _impl) external returns (bool) {
		(bool ok, bytes memory ret) =
			_impl.delegatecall(abi.encodeWithSelector(this.originIsRoot.selector));
		require(ok, "delegate originIsRoot failed");
		return abi.decode(ret, (bool));
	}
}
