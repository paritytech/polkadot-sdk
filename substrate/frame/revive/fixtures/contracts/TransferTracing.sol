// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

/// @notice Test fixture for exercising transfer-tracing edge cases in eth_simulateV1.
contract TransferTracing {
	/// @notice Forward `amount` to `target`, keeping any remainder.
	function forwardValue(address target, uint256 amount) external payable {
		(bool ok, ) = target.call{value: amount}("");
		require(ok, "forwardValue: failed");
	}

	/// @notice Make two value transfers in sequence.
	function multiTransfer(
		address target1, uint256 amount1,
		address target2, uint256 amount2
	) external payable {
		(bool ok1, ) = target1.call{value: amount1}("");
		require(ok1, "multiTransfer: t1 failed");
		(bool ok2, ) = target2.call{value: amount2}("");
		require(ok2, "multiTransfer: t2 failed");
	}

	/// @notice Nested forward: calls `middleman.forwardValue(finalTarget, innerAmount)`
	///         with `middlemanAmount` attached.
	function nestedForward(
		address middleman,
		address finalTarget,
		uint256 middlemanAmount,
		uint256 innerAmount
	) external payable {
		bytes memory data = abi.encodeWithSelector(
			this.forwardValue.selector,
			finalTarget,
			innerAmount
		);
		(bool ok, ) = middleman.call{value: middlemanAmount}(data);
		require(ok, "nestedForward: failed");
	}

	/// @notice Delegate call — does NOT transfer value in the delegate frame.
	function delegateForward(address target, bytes calldata data) external payable {
		(bool ok, ) = target.delegatecall(data);
		require(ok, "delegateForward: failed");
	}

	/// @notice Always reverts after receiving value.
	function revertAfterReceive() external payable {
		revert("intentional revert");
	}

	/// @notice Calls `target.revertAfterReceive()` with `amount`.
	///         The inner call reverts but this function SUCCEEDS (catches the revert).
	function callAndCatchRevert(address target, uint256 amount) external payable {
		target.call{value: amount}(
			abi.encodeWithSelector(this.revertAfterReceive.selector)
		);
		// intentionally ignore success — outer call succeeds
	}

	/// @notice Transfers `amount` to `target`, then REVERTS the entire call.
	function transferThenRevert(address target, uint256 amount) external payable {
		(bool ok, ) = target.call{value: amount}("");
		require(ok, "transferThenRevert: transfer failed");
		revert("revert after successful transfer");
	}

	receive() external payable {}
	fallback() external payable {}
}
