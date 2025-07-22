// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Dummy {
    constructor() payable {}
    fallback() external payable {}
}