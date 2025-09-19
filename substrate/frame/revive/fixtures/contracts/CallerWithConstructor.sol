// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

contract CallerWithConstructor {
    CallerWithConstructorCallee callee;

    constructor() {
      callee = new CallerWithConstructorCallee();
    }

    function callBar() public view returns (uint) {
      return callee.bar();
    }
}

contract CallerWithConstructorCallee {
    function bar() public pure returns (uint) {
      return 42;
    }
}

