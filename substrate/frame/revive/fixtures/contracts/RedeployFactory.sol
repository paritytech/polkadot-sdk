// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

/// Deploys `RedeployChild` with CREATE2 at a fixed salt. A factory re-created at the same address
/// therefore deploys its child at the same address again. Both contracts terminate themselves
/// through the system precompile, so termination works across transactions. `selfDestruct` uses
/// the SELFDESTRUCT opcode, which only deletes a contract created in the same transaction.
contract RedeployFactory {
	function deployChild() external returns (address) {
		return address(new RedeployChild{salt: bytes32(0)}());
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

	function selfDestruct(address beneficiary) external {
		assembly {
			selfdestruct(beneficiary)
		}
	}
}

/// Holds two storage values in separate slots and terminates itself on request.
contract RedeployChild {
	uint256 public value;
	uint256 public other;

	function setValue(uint256 newValue) external {
		value = newValue;
	}

	function setOther(uint256 newValue) external {
		other = newValue;
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

	function selfDestruct(address beneficiary) external {
		assembly {
			selfdestruct(beneficiary)
		}
	}
}

/// Deploys `RedeployFactory` with CREATE2 at a fixed salt and lets it deploy its child.
contract RedeployDeployer {
	/// Creates the factory and its child, stores `value` in the child and self-destructs both.
	/// Everything happens in one transaction, so SELFDESTRUCT deletes both contracts.
	function createAndDestroy(uint256 value) external returns (address factory, address child) {
		(RedeployFactory f, RedeployChild c) = create();
		c.setValue(value);
		c.selfDestruct(address(this));
		f.selfDestruct(address(this));
		return (address(f), address(c));
	}

	function createPair() external returns (address factory, address child) {
		(RedeployFactory f, RedeployChild c) = create();
		return (address(f), address(c));
	}

	function create() private returns (RedeployFactory f, RedeployChild c) {
		f = new RedeployFactory{salt: bytes32(0)}();
		c = RedeployChild(f.deployChild());
	}
}
