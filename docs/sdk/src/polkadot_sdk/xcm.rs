#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]

#[cfg(test)]
mod tests {
	use xcm::latest::prelude::*;

	#[docify::export]
	#[test]
	fn example_transfer() {
		let _transfer_program = Xcm::<()>(vec![
			WithdrawAsset((Here, 100u128).into()),
			BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { id: [0u8; 32].into(), network: None }.into(),
			},
		]);
	}
}

#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]

#[cfg(test)]
mod tests {
	use xcm::latest::prelude::*;

	#[docify::export]
	#[test]
	fn example_transfer() {
		let _transfer_program = Xcm::<()>(vec![
			WithdrawAsset((Here, 100u128).into()),
			BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { id: [0u8; 32].into(), network: None }.into(),
			},
		]);
	}
}


#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]

#[cfg(test)]
mod tests {
	use xcm::latest::prelude::*;

	#[docify::export]
	#[test]
	fn example_transfer() {
		let _transfer_program = Xcm::<()>(vec![
			WithdrawAsset((Here, 100u128).into()),
			BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { id: [0u8; 32].into(), network: None }.into(),
			},
		]);
	}
}

#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]

#[cfg(test)]
mod tests {
	use xcm::latest::prelude::*;

	#[docify::export]
	#[test]
	fn example_transfer() {
		let _transfer_program = Xcm::<()>(vec![
			WithdrawAsset((Here, 100u128).into()),
			BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { id: [0u8; 32].into(), network: None }.into(),
			},
		]);
	}
}



// [``]: 