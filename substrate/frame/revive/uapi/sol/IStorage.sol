// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

address constant STORAGE_ADDR = 0x0000000000000000000000000000000000000901;

interface IStorage {
	/// Clear the value at the given key in the contract storage.
	///
	/// # Important
	///
	/// This function can only be called via a delegate call! For Solidity, the low level
	/// `delegatecall` function has to be used. For languages that use the FFI
	/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
	///
	/// # Parameters
	///
	/// - `key`: The storage key.
	///
	/// # Return
	///
	/// If no entry existed for this key, `containedKey` is `false` and
	/// `valueLen` is `0`.
	function clearStorage(uint32 flags, bool isFixedKey, bytes memory key)
		external returns (bool containedKey, uint valueLen);

	/// Checks whether there is a value stored under the given key.
	///
	/// The key length must not exceed the maximum defined by the contracts module parameter.
	///
	/// # Important
	///
	/// This function can only be called via a delegate call! For Solidity, the low level
	/// `delegatecall` function has to be used. For languages that use the FFI
	/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
	///
	/// # Parameters
	///
	/// - `key`: The storage key.
	///
	/// # Return
	///
	/// Returns the size of the pre-existing value at the specified key.
	/// If no entry exists for this key `containedKey` is `false` and
	/// `valueLen` is `0`.
	function containsStorage(uint32 flags, bool isFixedKey, bytes memory key)
		external view returns (bool containedKey, uint valueLen);

	/// Retrieve and remove the value under the given key from storage.
	///
	/// # Important
	///
	/// This function can only be called via a delegate call! For Solidity, the low level
	/// `delegatecall` function has to be used. For languages that use the FFI
	/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
	///
	/// # Parameters
	///
	/// - `key`: The storage key.
	///
	/// # Errors
	///
	/// Returns empty bytes if no value was found under `key`.
	function takeStorage(uint32 flags, bool isFixedKey, bytes memory key)
		external returns (bytes memory);
}
