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

	/// Delegate-call into `impl.callerIsRoot()`, the same shape as an upgradeable proxy
	/// dispatching into its implementation.
	function delegateCallerIsRoot(address _impl) external returns (bool) {
		(bool ok, bytes memory ret) =
			_impl.delegatecall(abi.encodeWithSelector(this.callerIsRoot.selector));
		require(ok, "delegate callerIsRoot failed");
		return abi.decode(ret, (bool));
	}

	/// Delegate-call into `lib.callCallerIsRoot(target)`: the delegated code runs as this
	/// contract and makes a regular call into `target.callerIsRoot()`, so the caller that
	/// `target` sees is this contract.
	function delegateThenCallCallerIsRoot(address lib, address target) external returns (bool) {
		(bool ok, bytes memory ret) =
			lib.delegatecall(abi.encodeWithSelector(this.callCallerIsRoot.selector, target));
		require(ok, "delegate callCallerIsRoot failed");
		return abi.decode(ret, (bool));
	}

	/// Delegate-call `data` into `_impl` and decode the returned `bool`. Passing the calldata of
	/// another `delegate*` function chains a second delegate call.
	function delegateBool(address _impl, bytes calldata data) external returns (bool) {
		(bool ok, bytes memory ret) = _impl.delegatecall(data);
		require(ok, "delegate failed");
		return abi.decode(ret, (bool));
	}

	/// Directly invoke `callerIsOrigin()` on the System precompile.
	function callerIsOrigin() external view returns (bool) {
		return ISystem(SYSTEM_ADDR).callerIsOrigin();
	}

	/// Regular contract call into `target.callerIsOrigin()`.
	function callCallerIsOrigin(address target) external view returns (bool) {
		return OriginIsRoot(target).callerIsOrigin();
	}

	/// Delegate-call into `impl.callerIsOrigin()`, the same shape as an upgradeable proxy
	/// dispatching into its implementation.
	function delegateCallerIsOrigin(address _impl) external returns (bool) {
		(bool ok, bytes memory ret) =
			_impl.delegatecall(abi.encodeWithSelector(this.callerIsOrigin.selector));
		require(ok, "delegate callerIsOrigin failed");
		return abi.decode(ret, (bool));
	}

	/// Delegate-call into `lib.callCallerIsOrigin(target)`: the delegated code runs as this
	/// contract and makes a regular call into `target.callerIsOrigin()`, so the caller that
	/// `target` sees is this contract.
	function delegateThenCallCallerIsOrigin(address lib, address target) external returns (bool) {
		(bool ok, bytes memory ret) =
			lib.delegatecall(abi.encodeWithSelector(this.callCallerIsOrigin.selector, target));
		require(ok, "delegate callCallerIsOrigin failed");
		return abi.decode(ret, (bool));
	}
}
