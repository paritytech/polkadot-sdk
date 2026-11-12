// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// The pallet-jamkb precompile (design §3.2.1). This prototype exposes the permanent release,
/// redemption and voluntary return calls; the lease and recovery calls are not implemented.
///
/// Every account is an AccountId32 passed as `bytes32`. For a contract, and any unmapped Ethereum
/// account, it is `H160 ++ 0xEE * 12`, which the System precompile's `toAccountId` returns.
/// On failure a call reverts with the pallet error name.
address constant JAMKB_ADDR = 0x0000000000000000000000000000000009030000;

interface IJamkb {
	enum AllocationState { Approved, Delivering, Delivered, Reclaiming, Closed }
	enum AllocationMode { Lease, Permanent }
	enum OperationState { NotFound, Requested, Confirmed, Failed }

	struct Allocation {
		uint64 id;
		AllocationState state;
		AllocationMode mode;
		uint32 target;
		uint128 amount;
		bytes32 approver;      // bytes32(0) for governance
		uint32 deliveredAt;    // 0: not delivered
		uint32 duration;       // 0 for Permanent
		uint32 validFrom;
		uint32 expiresAt;
	}

	// -- Permanent release --
	function executeRelease(uint64 allocationId) external returns (uint64 opId);
	function redeem(uint128 amount, uint32 target, bytes32[4] calldata memo)
		external returns (uint64 opId);

	// -- Returns --
	function claim(bytes32 beneficiary) external returns (uint128 claimed);

	// -- Shared --
	function cancelAllocation(uint64 allocationId) external;

	// -- Reads. allocation() reverts for an unknown id. --
	function allocation(uint64 allocationId) external view returns (Allocation memory);
	/// `NotFound` for an id the pallet does not hold: settled, or never issued.
	function operationState(uint64 opId) external view returns (OperationState);
	function claimableOf(bytes32 beneficiary) external view returns (uint128);
	function currentTimeslot() external view returns (uint32);
}
