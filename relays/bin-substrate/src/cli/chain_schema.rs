// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#[cfg(test)]
mod tests {
	use sp_core::Pair;
	use substrate_relay_helper::cli::chain_schema::TargetSigningParams;

	#[test]
	fn reads_suri_from_file() {
		const ALICE: &str = "//Alice";
		const BOB: &str = "//Bob";
		const ALICE_PASSWORD: &str = "alice_password";
		const BOB_PASSWORD: &str = "bob_password";

		let alice: sp_core::sr25519::Pair = Pair::from_string(ALICE, Some(ALICE_PASSWORD)).unwrap();
		let bob: sp_core::sr25519::Pair = Pair::from_string(BOB, Some(BOB_PASSWORD)).unwrap();
		let bob_with_alice_password =
			sp_core::sr25519::Pair::from_string(BOB, Some(ALICE_PASSWORD)).unwrap();

		let temp_dir = tempfile::tempdir().unwrap();
		let mut suri_file_path = temp_dir.path().to_path_buf();
		let mut password_file_path = temp_dir.path().to_path_buf();
		suri_file_path.push("suri");
		password_file_path.push("password");
		std::fs::write(&suri_file_path, BOB.as_bytes()).unwrap();
		std::fs::write(&password_file_path, BOB_PASSWORD.as_bytes()).unwrap();

		// when both seed and password are read from file
		assert_eq!(
			TargetSigningParams {
				target_signer: Some(ALICE.into()),
				target_signer_password: Some(ALICE_PASSWORD.into()),

				target_signer_file: None,
				target_signer_password_file: None,

				target_transactions_mortality: None,
			}
			.to_keypair::<relay_polkadot_client::Polkadot>()
			.map(|p| p.public())
			.map_err(drop),
			Ok(alice.public()),
		);

		// when both seed and password are read from file
		assert_eq!(
			TargetSigningParams {
				target_signer: None,
				target_signer_password: None,

				target_signer_file: Some(suri_file_path.clone()),
				target_signer_password_file: Some(password_file_path.clone()),

				target_transactions_mortality: None,
			}
			.to_keypair::<relay_polkadot_client::Polkadot>()
			.map(|p| p.public())
			.map_err(drop),
			Ok(bob.public()),
		);

		// when password are is overriden by cli option
		assert_eq!(
			TargetSigningParams {
				target_signer: None,
				target_signer_password: Some(ALICE_PASSWORD.into()),

				target_signer_file: Some(suri_file_path.clone()),
				target_signer_password_file: Some(password_file_path.clone()),

				target_transactions_mortality: None,
			}
			.to_keypair::<relay_polkadot_client::Polkadot>()
			.map(|p| p.public())
			.map_err(drop),
			Ok(bob_with_alice_password.public()),
		);

		// when both seed and password are overriden by cli options
		assert_eq!(
			TargetSigningParams {
				target_signer: Some(ALICE.into()),
				target_signer_password: Some(ALICE_PASSWORD.into()),

				target_signer_file: Some(suri_file_path),
				target_signer_password_file: Some(password_file_path),

				target_transactions_mortality: None,
			}
			.to_keypair::<relay_polkadot_client::Polkadot>()
			.map(|p| p.public())
			.map_err(drop),
			Ok(alice.public()),
		);
	}
}
