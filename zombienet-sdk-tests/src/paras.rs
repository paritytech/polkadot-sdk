//! Helper functions to assert on parachains (e.g registration / height)

use crate::{polkadot, Error};
use std::time::Duration;
use log::trace;

use polkadot::runtime_types::polkadot_parachain_primitives::primitives::Id;
use parity_scale_codec::{Decode, Encode};
use anyhow::anyhow;

const LOG_TARGET: &str = "zombienet-sdk::helpers::paras";

async fn query_paras<Config: subxt::Config>(client: &subxt::OnlineClient<Config>) -> Result<Vec<Id>,Error> {
    let paras = client
    .storage()
    .at_latest()
    .await?
    .fetch(&polkadot::storage().paras().parachains())
    .await?.unwrap_or_default();
	Ok(paras)
}

//
async fn query_head<Config: subxt::Config>(client: &subxt::OnlineClient<Config>, para_id: u32) -> Result<HeadData,Error> {
	let head = client
    .storage()
    .at_latest()
    .await?
    .fetch(&polkadot::storage().paras().heads(Id(para_id)))
    .await?.ok_or(anyhow!("None value in storage paras.heads"))?;


	Ok(HeadData::decode(&mut &head.0[..])?)
}

// // TODO: replace with subxt
// pub async fn query_para_head_from_relay(node: &NetworkNode, para_id: u32, user_types: Option<serde_json::Value>) -> Result<u64, Error> {
//     // run pjs with code
//     let query_para_head = r#"
// 	const paraId = arguments[0];
// 	const optHeadData = await api.query.paras.heads<Option<HeadData>>(paraId);
// 	console.log(optHeadData);

// 	if (optHeadData?.isSome) {
// 	  const header = api.createType("Header", optHeadData.unwrap().toHex());
// 	  console.log(header);
// 	  const headerStr = JSON.stringify(header?.toHuman(), null, 2);
// 	  console.log(headerStr);


// 	  const headerObj = JSON.parse(headerStr);
// 	  const blockNumber = parseInt(headerObj["number"].replace(",", ""));

// 	  return blockNumber;
// 	} else {
// 	  return 10;
// 	}
//     "#;

//     let para_head = node.pjs(query_para_head, vec![json!(para_id)], user_types).await??;
// 	println!("{para_head:?}");
// 	Ok(para_head.as_u64().unwrap_or_default())
// }


/// Head data for this parachains using adder/undying collator.
#[derive(Default, Clone, Hash, Eq, PartialEq, Encode, Decode, Debug)]
pub struct HeadData {
	/// Block number
	pub number: u64,
	/// parent block keccak256
	pub parent_hash: [u8; 32],
	/// hash of post-execution state.
	pub post_state: [u8; 32],
}

// Read the parachain heads from relaychain and decode into the simple struct used by
// test collators (adder/undying). NOTE: if you are using other type of collator you should use
// the metrics provider by the prometheus endpoint of the collator.
// TODO: impl decoding of Head for cumulus collators.
pub async fn wait_para_block_height_from_heads<Config: subxt::Config>(
	client: &subxt::OnlineClient<Config>,
	para_id: u32,
	cmp: impl Fn(u64) -> bool,
	timeout_secs: Option<u64>
) -> Result<(),Error> {
	let mut head = query_head(client, para_id).await?;
	if let Some(timeout) = timeout_secs {
		let res = tokio::time::timeout(
			Duration::from_secs(timeout),
			async {
				loop {
					if cmp(head.number as u64) {
						break;
					} else {
						tokio::time::sleep(Duration::from_secs(1)).await;
						head = query_head(client, para_id).await?;
						trace!(
                            target: LOG_TARGET,
                            "Block # {}", head.number
                        );
					}
				}

				Ok(())
			}
		).await?;
		res
	} else {
		if !cmp(head.number.into()) {
			let err: anyhow::Error = anyhow!(
				format!("Current block height {} not pass the cmp fn", head.number)
			);
			Err(err)?
		} else {
			Ok(())
		}
	}
}


pub async fn wait_is_registered<Config: subxt::Config>(client: &subxt::OnlineClient<Config>, para_number: u32, timeout_secs: Option<u64>) -> Result<bool, Error> {
	let res = if let Some(timeout) = timeout_secs {
		tokio::time::timeout(
			Duration::from_secs(timeout),
			async  {
				let mut paras = query_paras(client).await?;
				loop {
					if paras.iter().any(|p| p.0 == para_number) {
						break;
					} else {
						tokio::time::sleep(Duration::from_secs(1)).await;
						paras = query_paras(client).await?;
					}
				}
				Ok(true)
			}
		).await?
	} else {
		let paras = query_paras(client).await?;
		Ok(paras.iter().any(|p| p.0 == para_number))
	};
	res
}
