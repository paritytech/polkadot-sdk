// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

/// Test fixture for storage that several callers pay for and can free again.
///
/// Each call to `growStorage` writes a fresh slot keyed by `msg.sender`, so distinct callers
/// grow distinct storage items rather than overwriting each other. On a `PGasDeposit` runtime
/// where a caller has no PGAS, the charge falls back to native currency and is recorded
/// per-payer in `NativeDepositOf[contract][caller]`. `clearStorage` deletes the caller's slot,
/// which refunds the caller's share of the deposit. `slotLength` returns the length of the slot
/// of `who`.
///
/// `terminate` invokes the `ISystem.terminate` precompile (not the EVM `SELFDESTRUCT` opcode).
contract ClearableContributorStorage {
	mapping(address => bytes) private slots;

	function growStorage() external {
		bytes memory payload = new bytes(64);
		for (uint i = 0; i < payload.length; i++) {
			payload[i] = 0xAB;
		}
		slots[msg.sender] = payload;
	}

	function clearStorage() external {
		delete slots[msg.sender];
	}

	function slotLength(address who) external view returns (uint256) {
		return slots[who].length;
	}

	function terminate(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}
