use codec::Encode;
use csv::ReaderBuilder;
use serde::Deserialize;
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{proving_trie::BasicProvingTrie, traits::BlakeTwo256, AccountId32};
use std::{
	collections::{BTreeMap, BTreeSet},
	error::Error,
	fs::File,
};

type AccountId = AccountId32;
type Balance = u128;
type DistributionTrie = BasicProvingTrie<BlakeTwo256, AccountId32, Balance>;

// Define the structure that matches the CSV file format
#[derive(Debug, Deserialize)]
struct CsvRecord {
	account: AccountId,
	balance: Balance,
	proof: bool,
}

// Function to read and process the CSV file
fn read_csv_and_process(file_path: &str) -> Result<(), Box<dyn Error>> {
	// Open the file
	let file = File::open(file_path)?;

	// Create a CSV reader
	let mut rdr = ReaderBuilder::new()
		.has_headers(true) // assuming the CSV file has headers
		.from_reader(file);

	let mut distribution = BTreeMap::<AccountId, Balance>::new();
	let mut proofs = BTreeSet::<AccountId>::new();

	// Iterate over each record
	for result in rdr.deserialize() {
		let record: CsvRecord = result?;
		distribution.insert(record.account.clone(), record.balance);
		if record.proof {
			proofs.insert(record.account.clone());
		}
	}

	let distribution_trie = DistributionTrie::generate_for(distribution)
		.map_err(|e| Box::<dyn Error>::from(<&'static str>::from(e)))?;

	for account in proofs.into_iter() {
		println!("\n\nCreating proof for account: {}", account);
		let balance = distribution_trie
			.query(account.clone())
			.ok_or("failed to find account in trie")?;
		println!("Amount Claimable: {}", balance);
		let proof = distribution_trie
			.create_single_value_proof(account)
			.map_err(|e| Box::<dyn Error>::from(<&'static str>::from(e)))?;
		println!("Proof Bytes: 0x{}", HexDisplay::from(&proof.encode()));
	}

	Ok(())
}

fn main() {
	// Path to your CSV file
	let file_path = "input.csv";

	if let Err(err) = read_csv_and_process(file_path) {
		eprintln!("Error processing CSV file: {}", err);
	}
}
