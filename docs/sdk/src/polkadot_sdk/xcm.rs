//! # XCM
//!
//!
//!
//!
//!
//! the virtual machine. These instructions aim to encompass all major things users typically do in
//!
//!
//! As long as an implementation of the XCVM is implemented, the same XCM program can be executed in
//!
//!
//!
//! - [`pallet_xcm`]: A FRAME pallet for interacting with the executor.
//!
//!
#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]
//!
//!

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

//!
//!
//!
//!
//!
//! the virtual machine. These instructions aim to encompass all major things users typically do in
//!
//!
//! As long as an implementation of the XCVM is implemented, the same XCM program can be executed in
//!
//!
//!
//! - [`pallet_xcm`]: A FRAME pallet for interacting with the executor.
//!
//!
#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]
//!
//!

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


//!
//!
//!
//!
//!
//! the virtual machine. These instructions aim to encompass all major things users typically do in
//!
//!
//! As long as an implementation of the XCVM is implemented, the same XCM program can be executed in
//!
//!
//!
//! - [`pallet_xcm`]: A FRAME pallet for interacting with the executor.
//!
//!
#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]
//!
//!

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

//!
//!
//!
//!
//!
//! the virtual machine. These instructions aim to encompass all major things users typically do in
//!
//!
//! As long as an implementation of the XCVM is implemented, the same XCM program can be executed in
//!
//!
//!
//! - [`pallet_xcm`]: A FRAME pallet for interacting with the executor.
//!
//!
#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]
//!
//!

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

// [`RFC process`]: https://github.com/paritytech/xcm-format/blob/master/proposals/0032-process.md
// [`pallet_xcm`]: pallet_xcm
// [`xcm`]: ::xcm
