// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Test utilities

#![cfg(test)]

use primitives::{traits::IdentityLookup, BuildStorage, Perbill};
use primitives::testing::{Digest, DigestItem, Header, UintAuthorityId, ConvertUintAuthorityId};
use substrate_primitives::{H256, Blake2Hasher};
use runtime_io;
use srml_support::impl_outer_origin;
use crate::{GenesisConfig, Module, Trait};

impl_outer_origin!{
	pub enum Origin for Test {}
}

// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Test;
impl consensus::Trait for Test {
	type Log = DigestItem;
	type SessionKey = UintAuthorityId;
	type InherentOfflineReport = ();
}
impl system::Trait for Test {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = ::primitives::traits::BlakeTwo256;
	type Digest = Digest;
	type AccountId = u64;
	type Lookup = IdentityLookup<u64>;
	type Header = Header;
	type Event = ();
	type Log = DigestItem;
}
impl balances::Trait for Test {
	type Balance = u64;
	type OnFreeBalanceZero = Staking;
	type OnNewAccount = ();
	type Event = ();
}
impl session::Trait for Test {
	type ConvertAccountIdToSessionKey = ConvertUintAuthorityId;
	type OnSessionChange = Staking;
	type Event = ();
}
impl timestamp::Trait for Test {
	type Moment = u64;
	type OnTimestampSet = ();
}
impl Trait for Test {
	type Currency = balances::Module<Self>;
	type OnRewardMinted = ();
	type Event = ();
}

pub struct ExtBuilder {
	existential_deposit: u64,
	session_length: u64,
	sessions_per_era: u64,
	current_era: u64,
	monied: bool,
	reward: u64,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: 0,
			session_length: 3,
			sessions_per_era: 3,
			current_era: 0,
			monied: true,
			reward: 10,
		}
	}
}

impl ExtBuilder {
	pub fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}
	pub fn session_length(mut self, session_length: u64) -> Self {
		self.session_length = session_length;
		self
	}
	pub fn sessions_per_era(mut self, sessions_per_era: u64) -> Self {
		self.sessions_per_era = sessions_per_era;
		self
	}
	pub fn _current_era(mut self, current_era: u64) -> Self {
		self.current_era = current_era;
		self
	}
	pub fn _monied(mut self, monied: bool) -> Self {
		self.monied = monied;
		self
	}
	pub fn reward(mut self, reward: u64) -> Self {
		self.reward = reward;
		self
	}
	pub fn build(self) -> runtime_io::TestExternalities<Blake2Hasher> {
		let (mut t, mut c) = system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let balance_factor = if self.existential_deposit > 0 {
			256
		} else {
			1
		};
		let _ = consensus::GenesisConfig::<Test>{
			code: vec![],
			authorities: vec![],
		}.assimilate_storage(&mut t, &mut c);
		let _ = session::GenesisConfig::<Test>{
			session_length: self.session_length,
			validators: vec![10, 20],
			keys: vec![],
		}.assimilate_storage(&mut t, &mut c);
		let _ = balances::GenesisConfig::<Test>{
			balances: if self.monied {
				if self.reward > 0 {
					vec![(1, 10 * balance_factor), (2, 20 * balance_factor), (3, 300 * balance_factor), (4, 400 * balance_factor), (10, balance_factor), (11, balance_factor * 1000), (20, balance_factor), (21, balance_factor * 2000)]
				} else {
					vec![(1, 10 * balance_factor), (2, 20 * balance_factor), (3, 300 * balance_factor), (4, 400 * balance_factor)]
				}
			} else {
				vec![(10, balance_factor), (11, balance_factor * 1000), (20, balance_factor), (21, balance_factor * 2000)]
			},
			existential_deposit: self.existential_deposit,
			transfer_fee: 0,
			creation_fee: 0,
			vesting: vec![],
		}.assimilate_storage(&mut t, &mut c);
		let _ = GenesisConfig::<Test>{
			sessions_per_era: self.sessions_per_era,
			current_era: self.current_era,
			stakers: vec![(11, 10, balance_factor * 1000), (21, 20, balance_factor * 2000)],
			validator_count: 2,
			minimum_validator_count: 0,
			bonding_duration: self.sessions_per_era * self.session_length * 3,
			session_reward: Perbill::from_millionths((1000000 * self.reward / balance_factor) as u32),
			offline_slash: if self.monied { Perbill::from_percent(40) } else { Perbill::zero() },
			current_session_reward: self.reward,
			current_offline_slash: 20,
			offline_slash_grace: 0,
			invulnerables: vec![],
		}.assimilate_storage(&mut t, &mut c);
		let _ = timestamp::GenesisConfig::<Test>{
			period: 5,
		}.assimilate_storage(&mut t, &mut c);
		t.into()
	}
}

pub type System = system::Module<Test>;
pub type Balances = balances::Module<Test>;
pub type Session = session::Module<Test>;
pub type Timestamp = timestamp::Module<Test>;
pub type Staking = Module<Test>;
