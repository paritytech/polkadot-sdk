//! Solidity ERC20 interface.

use alloy_core::sol;

sol! {
	interface IERC20 {
		function totalSupply() public view virtual returns (uint256);
		function balanceOf(address account) public view virtual returns (uint256);
		function transfer(address to, uint256 value) public virtual returns (bool);
	}
}
