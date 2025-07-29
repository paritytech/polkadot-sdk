// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {

	function add(uint a, uint b) public view returns (uint) {
		return a + b;
	}

    function mul(uint a, uint b) public view returns (uint) {
		return a * b;
	}

    function sub(uint a, uint b) public view returns (uint) {
		return a - b;
	}

    function div(uint a, uint b) public view returns (uint) {
		return a / b;
	}

    function sdiv(int a, int b) public view returns (int) {
		return a / b;
	}

    function rem(uint a, uint b) public view returns (uint) {
		return a % b;
	}

    function smod(int a, int b) public view returns (int) {
		return a % b;
	}

    // MOD instruction - unsigned modulo (alternative name to avoid Rust keyword conflict)
    function umod(uint a, uint b) public view returns (uint) {
		return a % b;
	}

    // ADDMOD instruction: (a + b) % n
    function addmod(uint a, uint b, uint n) public view returns (uint) {
		return (a + b) % n;
	}

    // MULMOD instruction: (a * b) % n  
    function mulmod(uint a, uint b, uint n) public view returns (uint) {
		return (a * b) % n;
	}

    // EXP instruction: a ** b (exponentiation)
    function exp(uint a, uint b) public view returns (uint) {
		return a ** b;
	}

}
