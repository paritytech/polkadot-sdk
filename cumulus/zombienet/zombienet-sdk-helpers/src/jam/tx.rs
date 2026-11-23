// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Extrinsics on a JAM para.
//!
//! A JAM collator's finality is a projection of JAM's accumulated parachain head and does not
//! advance while the service's head is unchanged, so inclusion in a best block is the strongest
//! signal a JAM para gives. Use [`crate::submit_extrinsic_and_wait_for_finalization_success`] on a
//! chain with real finality.

use crate::{
	find_event_and_decode_fields,
	jam::{coretime::sudo_call, JamConfig},
	PolkadotLikeConfig,
};
use anyhow::anyhow;
use zombienet_sdk::subxt::{
	config::polkadot::PolkadotExtrinsicParamsBuilder,
	ext::scale_value::{value, Value},
	tx::{signer::Signer, DynamicPayload, SubmittableTransaction, TxStatus},
	utils::H256,
	OnlineClient,
};

/// Submits the given `call` as a signed transaction and waits until it is included in a **best**
/// block whose dispatch succeeded.
///
/// The nonce is read at the best block too. subxt reads it at the finalized block by default,
/// which lags behind on a JAM para: a second call right after the first would reuse its nonce.
pub async fn submit_extrinsic_and_wait_for_best_block_success<
	C: PolkadotLikeConfig,
	S: Signer<C>,
>(
	client: &OnlineClient<C>,
	call: &DynamicPayload,
	signer: &S,
) -> Result<H256, anyhow::Error> {
	let best = client
		.blocks()
		.subscribe_best()
		.await?
		.next()
		.await
		.ok_or_else(|| anyhow!("the best block stream ended before yielding a block"))??;
	let nonce = best.account_nonce(&signer.account_id()).await?;
	let extensions = PolkadotExtrinsicParamsBuilder::<C>::new().immortal().nonce(nonce).build();

	log::info!("Submitting transaction...");

	let tx = client.tx().create_signed(call, signer, extensions).await?;

	submit_tx_and_wait_for_best_block(tx).await
}

/// Submit `tx` and return the hash of the best block that included it successfully.
async fn submit_tx_and_wait_for_best_block<C: PolkadotLikeConfig>(
	tx: SubmittableTransaction<C, OnlineClient<C>>,
) -> Result<H256, anyhow::Error> {
	log::info!("Submitting transaction: {:?}", tx.hash());

	let mut tx = tx.submit_and_watch().await?;

	while let Some(status) = tx.next().await.transpose()? {
		match status {
			TxStatus::InBestBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				log::info!("[Best] In block: {:#?}", tx_in_block.block_hash());
				return Ok(tx_in_block.block_hash());
			},
			TxStatus::InFinalizedBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				log::info!("[Finalized] In block: {:#?}", tx_in_block.block_hash());
				return Ok(tx_in_block.block_hash());
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow!("Error submitting tx: {message}"));
			},
			_ => continue,
		}
	}

	Err(anyhow!("Transaction event stream ended without being included in a best block"))
}

/// Arms a JAM runtime upgrade to `code`: a sudo `JamSystem::schedule_code_upgrade_hash` with the
/// code's 32-byte hash and length. The code itself never travels on chain, a multi-MB extrinsic
/// would trap the PolkaVM runtime. Returns the block that armed it.
pub async fn schedule_code_upgrade(
	client: &OnlineClient<JamConfig>,
	code: &[u8],
) -> Result<H256, anyhow::Error> {
	let code_len = code.len() as u32;
	let call = zombienet_sdk::subxt::tx::dynamic(
		"JamSystem",
		"schedule_code_upgrade_hash",
		vec![Value::from_bytes(sp_crypto_hashing::blake2_256(code)), value!(code_len)],
	);
	log::info!("Sending sudo(schedule_code_upgrade_hash) for {code_len} bytes");
	let block = submit_extrinsic_and_wait_for_best_block_success(
		client,
		&sudo_call(call),
		&zombienet_sdk::subxt_signer::sr25519::dev::alice(),
	)
	.await?;

	// `sudo` reports the inner dispatch only through `Sudid`; the extrinsic itself succeeds
	// either way.
	let events = client.blocks().at(block).await?.events().await?;
	let sudid: Vec<Result<(), sp_runtime::DispatchError>> =
		find_event_and_decode_fields(&events, "Sudo", "Sudid")?;
	match sudid.first() {
		Some(Ok(())) => Ok(block),
		Some(Err(e)) => Err(anyhow!("sudo(schedule_code_upgrade_hash) failed: {e:?}")),
		None => Err(anyhow!("no Sudid event in block {block:?}")),
	}
}
