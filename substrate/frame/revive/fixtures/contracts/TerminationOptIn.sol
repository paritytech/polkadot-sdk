// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

/// The constructor calls `ISystem.allowTermination` as `mode` selects: 0 not at all, 1 by a call,
/// 2 by a delegate call, 3 by a delegate call to `allowTermination` of `helper`, 4 by a call
/// followed by a revert. With 5 it uses the `mode` of its creator, a `TerminationOptInFactory`.
contract TerminationOptIn {
	constructor(uint8 mode, address helper) payable {
		if (mode == 5) mode = TerminationOptInFactory(msg.sender).mode();
		if (mode == 1 || mode == 4) ISystem(SYSTEM_ADDR).allowTermination();
		if (mode == 2 || mode == 3) {
			bytes memory data = abi.encodeCall(ISystem.allowTermination, ());
			(bool ok, bytes memory err) = (mode == 2 ? SYSTEM_ADDR : helper).delegatecall(data);
			if (!ok) assembly { revert(add(err, 0x20), mload(err)) }
		}
		require(mode != 4, "constructor reverted");
	}

	function allowTermination() external {
		ISystem(SYSTEM_ADDR).allowTermination();
	}

	function terminate(address beneficiary) external {
		ISystem(SYSTEM_ADDR).terminate(beneficiary);
	}

	function selfDestruct(address beneficiary) external {
		assembly { selfdestruct(beneficiary) }
	}
}

/// The constructor creates a `TerminationOptIn` with mode 1. `create` creates one with CREATE2 at
/// a fixed salt, so always at the same address, and returns the zero address if that fails.
contract TerminationOptInFactory {
	uint8 public mode;
	address public child = address(new TerminationOptIn(1, address(0)));

	/// Runs `SELFDESTRUCT` in the created contract if `beneficiary` is not the zero address.
	function create(uint8 mode_, address beneficiary) external returns (address created) {
		mode = mode_;
		try new TerminationOptIn{salt: 0}(5, address(0)) returns (TerminationOptIn c) {
			created = address(c);
			if (beneficiary != address(0)) c.selfDestruct(beneficiary);
		} catch {}
	}
}
