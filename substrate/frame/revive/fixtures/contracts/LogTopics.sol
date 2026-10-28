// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// Emits LOG0 to LOG4 in order, so a test can compare their charges by topic count.
contract LogTopics {
    /// Every log carries 32 bytes of data.
    function logsWithData() public {
        assembly {
            log0(0x01, 0x20)
            log1(0x02, 0x20, 0x11)
            log2(0x03, 0x20, 0x22, 0x33)
            log3(0x04, 0x20, 0x44, 0x55, 0x66)
            log4(0x05, 0x20, 0x77, 0x88, 0x99, 0xaa)
        }
    }

    /// Every log has an empty data field.
    function logsWithoutData() public {
        assembly {
            log0(0, 0)
            log1(0, 0, 0x11)
            log2(0, 0, 0x22, 0x33)
            log3(0, 0, 0x44, 0x55, 0x66)
            log4(0, 0, 0x77, 0x88, 0x99, 0xaa)
        }
    }
}
