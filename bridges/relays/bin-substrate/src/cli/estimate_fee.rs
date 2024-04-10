// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use crate::{
	chains::{
		millau_headers_to_rialto::MillauToRialtoCliBridge,
		millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
		rialto_headers_to_millau::RialtoToMillauCliBridge,
		rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
	},
	cli::{
		bridge::{FullBridge, MessagesCliBridge},
		chain_schema::*,
		relay_headers_and_messages::CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
		Balance, HexBytes, HexLaneId,
	},
};
use async_trait::async_trait;
use bp_runtime::BalanceOf;
use codec::{Decode, Encode};
use relay_substrate_client::{Chain, ChainBase};
use sp_runtime::FixedU128;
use std::fmt::Display;
use structopt::StructOpt;
use strum::VariantNames;
use substrate_relay_helper::helpers::tokens_conversion_rate_from_metrics;

/// Estimate Delivery & Dispatch Fee command.
#[derive(StructOpt, Debug, PartialEq)]
pub struct EstimateFee {
	/// A bridge instance to encode call for.
	#[structopt(possible_values = FullBridge::VARIANTS, case_insensitive = true)]
	bridge: FullBridge,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	/// Hex-encoded id of lane that will be delivering the message.
	#[structopt(long, default_value = "00000000")]
	lane: HexLaneId,
	/// A way to override conversion rate between bridge tokens.
	///
	/// If not specified, conversion rate from runtime storage is used. It may be obsolete and
	/// your message won't be relayed.
	#[structopt(long)]
	conversion_rate_override: Option<ConversionRateOverride>,
	/// Payload to send over the bridge.
	#[structopt(flatten)]
	payload: crate::cli::encode_message::Message,
}

/// A way to override conversion rate between bridge tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConversionRateOverride {
	/// The actual conversion rate is computed in the same way how rate metric works.
	Metric,
	/// The actual conversion rate is specified explicitly.
	Explicit(f64),
}

impl std::str::FromStr for ConversionRateOverride {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.to_lowercase() == "metric" {
			return Ok(ConversionRateOverride::Metric)
		}

		f64::from_str(s)
			.map(ConversionRateOverride::Explicit)
			.map_err(|e| format!("Failed to parse '{e:?}'. Expected 'metric' or explicit value"))
	}
}

#[async_trait]
trait FeeEstimator: MessagesCliBridge
where
	<Self::Source as ChainBase>::Balance: Display + Into<u128>,
{
	async fn estimate_fee(data: EstimateFee) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let lane = data.lane.into();
		let payload =
			crate::cli::encode_message::encode_message::<Self::Source, Self::Target>(&data.payload)
				.map_err(|e| anyhow::format_err!("{:?}", e))?;

		let fee = estimate_message_delivery_and_dispatch_fee::<Self::Source, Self::Target, _>(
			&source_client,
			data.conversion_rate_override,
			Self::ESTIMATE_MESSAGE_FEE_METHOD,
			lane,
			&payload,
		)
		.await?;

		log::info!(target: "bridge", "Fee: {:?}", Balance(fee.into()));
		println!("{fee}");
		Ok(())
	}
}

impl FeeEstimator for MillauToRialtoCliBridge {}
impl FeeEstimator for RialtoToMillauCliBridge {}
impl FeeEstimator for MillauToRialtoParachainCliBridge {}
impl FeeEstimator for RialtoParachainToMillauCliBridge {}

impl EstimateFee {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			FullBridge::MillauToRialto => MillauToRialtoCliBridge::estimate_fee(self),
			FullBridge::RialtoToMillau => RialtoToMillauCliBridge::estimate_fee(self),
			FullBridge::MillauToRialtoParachain =>
				MillauToRialtoParachainCliBridge::estimate_fee(self),
			FullBridge::RialtoParachainToMillau =>
				RialtoParachainToMillauCliBridge::estimate_fee(self),
			FullBridge::BridgeHubRococoToBridgeHubWococo |
			FullBridge::BridgeHubWococoToBridgeHubRococo =>
				unimplemented!("TODO: (EstimateFee) do we need it or refactor or remove?"),
		}
		.await
	}
}

/// The caller may provide target to source tokens conversion rate override to use in fee
/// computation.
pub(crate) async fn estimate_message_delivery_and_dispatch_fee<
	Source: Chain,
	Target: Chain,
	P: Clone + Encode,
>(
	client: &relay_substrate_client::Client<Source>,
	conversion_rate_override: Option<ConversionRateOverride>,
	estimate_fee_method: &str,
	lane: bp_messages::LaneId,
	payload: &P,
) -> anyhow::Result<BalanceOf<Source>> {
	// actual conversion rate CAN be lesser than the rate stored in the runtime. So we may try to
	// pay lesser fee for the message delivery. But in this case, message may be rejected by the
	// lane. So we MUST use the larger of two fees - one computed with stored fee and the one
	// computed with actual fee.

	let conversion_rate_override =
		match (conversion_rate_override, Source::TOKEN_ID, Target::TOKEN_ID) {
			(Some(ConversionRateOverride::Explicit(v)), _, _) => {
				let conversion_rate_override = FixedU128::from_float(v);
				log::info!(
					target: "bridge",
					"{} -> {} conversion rate override: {:?} (explicit)",
					Target::NAME,
					Source::NAME,
					conversion_rate_override.to_float(),
				);
				Some(conversion_rate_override)
			},
			(
				Some(ConversionRateOverride::Metric),
				Some(source_token_id),
				Some(target_token_id),
			) => {
				let conversion_rate_override =
					tokens_conversion_rate_from_metrics(target_token_id, source_token_id).await?;
				// So we have current actual conversion rate and rate that is stored in the runtime.
				// And we may simply choose the maximal of these. But what if right now there's
				// rate update transaction on the way, that is updating rate to 10 seconds old
				// actual rate, which is bigger than the current rate? Then our message will be
				// rejected.
				//
				// So let's increase the actual rate by the same value that the conversion rate
				// updater is using.
				let increased_conversion_rate_override = FixedU128::from_float(
					conversion_rate_override * (1.0 + CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO),
				);
				log::info!(
					target: "bridge",
					"{} -> {} conversion rate override: {} (value from metric - {})",
					Target::NAME,
					Source::NAME,
					increased_conversion_rate_override.to_float(),
					conversion_rate_override,
				);
				Some(increased_conversion_rate_override)
			},
			_ => None,
		};

	let without_override = do_estimate_message_delivery_and_dispatch_fee(
		client,
		estimate_fee_method,
		lane,
		payload,
		None,
	)
	.await?;
	let with_override = do_estimate_message_delivery_and_dispatch_fee(
		client,
		estimate_fee_method,
		lane,
		payload,
		conversion_rate_override,
	)
	.await?;
	let maximal_fee = std::cmp::max(without_override, with_override);

	log::info!(
		target: "bridge",
		"Estimated message fee: {:?} = max of {:?} (without rate override) and {:?} (with override to {:?})",
		maximal_fee,
		without_override,
		with_override,
		conversion_rate_override,
	);

	Ok(maximal_fee)
}

/// Estimate message delivery and dispatch fee with given conversion rate override.
async fn do_estimate_message_delivery_and_dispatch_fee<Source: Chain, P: Encode>(
	client: &relay_substrate_client::Client<Source>,
	estimate_fee_method: &str,
	lane: bp_messages::LaneId,
	payload: &P,
	conversion_rate_override: Option<FixedU128>,
) -> anyhow::Result<BalanceOf<Source>> {
	let encoded_response = client
		.state_call(
			estimate_fee_method.into(),
			(lane, payload, conversion_rate_override).encode().into(),
			None,
		)
		.await?;
	let decoded_response: Option<BalanceOf<Source>> = Decode::decode(&mut &encoded_response.0[..])
		.map_err(relay_substrate_client::Error::ResponseParseFailed)?;
	let fee = decoded_response.ok_or_else(|| {
		anyhow::format_err!("Unable to decode fee from: {:?}", HexBytes(encoded_response.to_vec()))
	})?;
	Ok(fee)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_parse_cli_options() {
		// when
		let res = EstimateFee::from_iter(vec![
			"estimate_fee",
			"rialto-to-millau",
			"--source-port",
			"1234",
			"--conversion-rate-override",
			"42.5",
			"raw",
			"1234",
		]);

		// then
		assert_eq!(
			res,
			EstimateFee {
				bridge: FullBridge::RialtoToMillau,
				lane: HexLaneId([0, 0, 0, 0]),
				conversion_rate_override: Some(ConversionRateOverride::Explicit(42.5)),
				source: SourceConnectionParams {
					source_host: "127.0.0.1".into(),
					source_port: 1234,
					source_secure: false,
					source_runtime_version: SourceRuntimeVersionParams {
						source_version_mode: RuntimeVersionType::Bundle,
						source_spec_version: None,
						source_transaction_version: None,
					}
				},
				payload: crate::cli::encode_message::Message::Raw {
					data: HexBytes(vec![0x12, 0x34])
				}
			}
		);
	}
}
