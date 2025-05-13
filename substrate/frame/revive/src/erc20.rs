// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Solidity ERC20 interface.

use alloy_core::sol;

sol! {
	interface IERC20 {
		function totalSupply() public view virtual returns (uint256);
		function balanceOf(address account) public view virtual returns (uint256);
		function transfer(address to, uint256 value) public virtual returns (bool);
	}
}
