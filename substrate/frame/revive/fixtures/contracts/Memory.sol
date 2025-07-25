// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Memory {
    function test_mload_mstore() public pure {
        uint256 stored;
        uint256 loaded;
        assembly {
            mstore(0x80, 0xdeadbeefcafebabe)
            loaded := mload(0x80)
        }
        assert(loaded == 0xdeadbeefcafebabe);
    }

    function test_mstore8() public pure {
        uint256 result;
        assembly {
            mstore(0x80, 0)
            mstore8(0x80, 0xab)
            result := mload(0x80)
        }
        assert(result == 0xab00000000000000000000000000000000000000000000000000000000000000);
    }

    function test_msize() public pure {
        uint256 size1;
        uint256 size2;
        assembly {
            size1 := msize()
            mstore(0x100, 0xdeadbeef)
            size2 := msize()
        }
        assert(size2 >= size1);
        assert(size2 >= 0x120);
    }

    function test_mcopy() public pure {
        uint256 src_data;
        uint256 dest_data;
        assembly {
            mstore(0x80, 0xdeadbeefcafebabe)
            mcopy(0xa0, 0x80, 0x20)
            src_data := mload(0x80)
            dest_data := mload(0xa0)
        }
        assert(src_data == dest_data);
        assert(dest_data == 0xdeadbeefcafebabe);
    }

    function test_memory_expansion() public pure {
        uint256 size_before;
        uint256 size_after;
        assembly {
            size_before := msize()
            mstore(0x200, 0x42)
            size_after := msize()
        }
        assert(size_after > size_before);
        assert(size_after >= 0x220);
    }
}