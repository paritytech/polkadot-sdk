// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.20;

contract Deposit {
  uint256 a;
  uint256 b;

  address immutable storagePrecompile = address(0x901);

  function clearStorageSlot(uint256 slot) internal {
    bytes memory key = abi.encodePacked(bytes32(slot));
    (bool _success, ) = storagePrecompile.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, key)
    );
  }

  function clearAll() external {
    clearStorageSlot(0);
    clearStorageSlot(1);
  }

  function setAndClear() external {
    a = 2;
    b = 3;

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = storagePrecompile.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }

  function callSetAndClear() external {
    this.setVars();

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = storagePrecompile.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }

  function setAndCallClear() external {
    a = 2;
    b = 3;

    this.clear();
  }


  function setVars() external {
    a = 2;
    b = 3;
  }

  function clear() external {
    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = storagePrecompile.delegatecall(
        abi.encodeWithSignature("clearStorage(uint32,bool,bytes)", 0, true, keyBytes)
    );
  }
}