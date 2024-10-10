use eth_rpc::{
	subxt_client::{self, build_params, CheckEvmGasParam, SrcChainConfig},
	MultiSignature,
};
use polkadot_sdk::sp_weights::Weight;
use subxt::{tx::Signer, Config, OnlineClient};
use subxt_signer::sr25519::dev;

static DUMMY_BYTES: &[u8] = include_bytes!("./dummy.polkavm");
struct SrcChainSigner(subxt_signer::sr25519::Keypair);
impl Signer<SrcChainConfig> for SrcChainSigner {
	fn account_id(&self) -> <SrcChainConfig as Config>::AccountId {
		self.0.public_key().into()
	}
	fn address(&self) -> <SrcChainConfig as Config>::Address {
		self.0.public_key().into()
	}

	fn sign(&self, signer_payload: &[u8]) -> <SrcChainConfig as Config>::Signature {
		MultiSignature::Sr25519(self.0.sign(signer_payload).0.into())
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let client = OnlineClient::<SrcChainConfig>::new().await?;

	println!("\n\n=== Deploying contract ===\n\n");

	let tx_payload = subxt_client::tx().revive().instantiate_with_code(
		0u32.into(),
		Weight::from_parts(2_000, 2_000).into(),
		10000000u32.into(),
		DUMMY_BYTES.to_vec(),
		vec![],
		None,
	);

	let res = client
		.tx()
		.sign_and_submit_default_then_watch(&tx_payload, &SrcChainSigner(dev::alice()))
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("Transaction finalized: {:?}", res.extrinsic_hash());

	Ok(())
}
