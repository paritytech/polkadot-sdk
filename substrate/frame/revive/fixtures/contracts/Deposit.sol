// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.20;

contract Deposit {
  uint256 a;
  uint256 b;

  function clearStorageSlot(uint256 slot) internal {
    address storagePrecompile = 0x0000000000000000000000000000000000000901;
    bytes memory key = abi.encodePacked(bytes32(slot));
    (bool _success, ) = storagePrecompile.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, key)
    );
  }

  function clear() external {
    clearStorageSlot(0);
    clearStorageSlot(1);
  }

  function c() external {
    address targetAddress = 0x0000000000000000000000000000000000000901;

    a = 2;
    b = 3;

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = targetAddress.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }

  function d() external {
    this.x();

    address targetAddress = 0x0000000000000000000000000000000000000901;
    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = targetAddress.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }

  function e() external {
    a = 2;
    b = 3;

    this.y();
  }


  function x() external {
    a = 2;
    b = 3;
  }

  function y() external {
    address targetAddress = 0x0000000000000000000000000000000000000901;

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = targetAddress.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }
}