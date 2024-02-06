// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	dispatch::GetDispatchInfo,
	ensure,
	traits::{Contains, ContainsPair, Defensive, Get, PalletsInfoAccess},
};
use parity_scale_codec::{Decode, Encode};
use sp_core::defer;
use sp_io::hashing::blake2_128;
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};
use sp_weights::Weight;
use xcm::latest::prelude::*;

pub mod traits;
use traits::{
	validate_export, AssetExchange, AssetLock, CallDispatcher, ClaimAssets, ConvertOrigin,
	DropAssets, Enact, ExportXcm, FeeManager, FeeReason, OnResponse, ProcessTransaction,
	Properties, ShouldExecute, TransactAsset, VersionChangeNotifier, WeightBounds, WeightTrader,
	XcmAssetTransfers,
};

mod assets;
pub use assets::AssetsInHolding;
mod config;
pub use config::Config;

/// A struct to specify how fees are being paid.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FeesMode {
	/// If true, then the fee assets are taken directly from the origin's on-chain account,
	/// otherwise the fee assets are taken from the holding register.
	///
	/// Defaults to false.
	pub jit_withdraw: bool,
}

const RECURSION_LIMIT: u8 = 10;

environmental::environmental!(recursion_count: u8);

/// The XCM executor.
pub struct XcmExecutor<Config: config::Config> {
	holding: AssetsInHolding,
	holding_limit: usize,
	context: XcmContext,
	original_origin: Location,
	trader: Config::Trader,
	/// The most recent error result and instruction index into the fragment in which it occurred,
	/// if any.
	error: Option<(u32, XcmError)>,
	/// The surplus weight, defined as the amount by which `max_weight` is
	/// an over-estimate of the actual weight consumed. We do it this way to avoid needing the
	/// execution engine to keep track of all instructions' weights (it only needs to care about
	/// the weight of dynamically determined instructions such as `Transact`).
	total_surplus: Weight,
	total_refunded: Weight,
	error_handler: Xcm<Config::RuntimeCall>,
	error_handler_weight: Weight,
	appendix: Xcm<Config::RuntimeCall>,
	appendix_weight: Weight,
	transact_status: MaybeErrorCode,
	fees_mode: FeesMode,
	_config: PhantomData<Config>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<Config: config::Config> XcmExecutor<Config> {
	pub fn holding(&self) -> &AssetsInHolding {
		&self.holding
	}
	pub fn set_holding(&mut self, v: AssetsInHolding) {
		self.holding = v
	}
	pub fn holding_limit(&self) -> &usize {
		&self.holding_limit
	}
	pub fn set_holding_limit(&mut self, v: usize) {
		self.holding_limit = v
	}
	pub fn origin(&self) -> &Option<Location> {
		&self.context.origin
	}
	pub fn set_origin(&mut self, v: Option<Location>) {
		self.context.origin = v
	}
	pub fn original_origin(&self) -> &Location {
		&self.original_origin
	}
	pub fn set_original_origin(&mut self, v: Location) {
		self.original_origin = v
	}
	pub fn trader(&self) -> &Config::Trader {
		&self.trader
	}
	pub fn set_trader(&mut self, v: Config::Trader) {
		self.trader = v
	}
	pub fn error(&self) -> &Option<(u32, XcmError)> {
		&self.error
	}
	pub fn set_error(&mut self, v: Option<(u32, XcmError)>) {
		self.error = v
	}
	pub fn total_surplus(&self) -> &Weight {
		&self.total_surplus
	}
	pub fn set_total_surplus(&mut self, v: Weight) {
		self.total_surplus = v
	}
	pub fn total_refunded(&self) -> &Weight {
		&self.total_refunded
	}
	pub fn set_total_refunded(&mut self, v: Weight) {
		self.total_refunded = v
	}
	pub fn error_handler(&self) -> &Xcm<Config::RuntimeCall> {
		&self.error_handler
	}
	pub fn set_error_handler(&mut self, v: Xcm<Config::RuntimeCall>) {
		self.error_handler = v
	}
	pub fn error_handler_weight(&self) -> &Weight {
		&self.error_handler_weight
	}
	pub fn set_error_handler_weight(&mut self, v: Weight) {
		self.error_handler_weight = v
	}
	pub fn appendix(&self) -> &Xcm<Config::RuntimeCall> {
		&self.appendix
	}
	pub fn set_appendix(&mut self, v: Xcm<Config::RuntimeCall>) {
		self.appendix = v
	}
	pub fn appendix_weight(&self) -> &Weight {
		&self.appendix_weight
	}
	pub fn set_appendix_weight(&mut self, v: Weight) {
		self.appendix_weight = v
	}
	pub fn transact_status(&self) -> &MaybeErrorCode {
		&self.transact_status
	}
	pub fn set_transact_status(&mut self, v: MaybeErrorCode) {
		self.transact_status = v
	}
	pub fn fees_mode(&self) -> &FeesMode {
		&self.fees_mode
	}
	pub fn set_fees_mode(&mut self, v: FeesMode) {
		self.fees_mode = v
	}
	pub fn topic(&self) -> &Option<[u8; 32]> {
		&self.context.topic
	}
	pub fn set_topic(&mut self, v: Option<[u8; 32]>) {
		self.context.topic = v;
	}
}

pub struct WeighedMessage<Call>(Weight, Xcm<Call>);
impl<C> PreparedMessage for WeighedMessage<C> {
	fn weight_of(&self) -> Weight {
		self.0
	}
}

impl<Config: config::Config> ExecuteXcm<Config::RuntimeCall> for XcmExecutor<Config> {
	type Prepared = WeighedMessage<Config::RuntimeCall>;
	fn prepare(
		mut message: Xcm<Config::RuntimeCall>,
	) -> Result<Self::Prepared, Xcm<Config::RuntimeCall>> {
		match Config::Weigher::weight(&mut message) {
			Ok(weight) => Ok(WeighedMessage(weight, message)),
			Err(_) => Err(message),
		}
	}
	fn execute(
		origin: impl Into<Location>,
		WeighedMessage(xcm_weight, mut message): WeighedMessage<Config::RuntimeCall>,
		id: &mut XcmHash,
		weight_credit: Weight,
	) -> Outcome {
		let origin = origin.into();
		log::trace!(
			target: "xcm::execute",
			"origin: {origin:?}, message: {message:?}, weight_credit: {weight_credit:?}",
		);
		let mut properties = Properties { weight_credit, message_id: None };
		if let Err(e) = Config::Barrier::should_execute(
			&origin,
			message.inner_mut(),
			xcm_weight,
			&mut properties,
		) {
			log::trace!(
				target: "xcm::execute",
				"Barrier blocked execution! Error: {e:?}. \
				 (origin: {origin:?}, message: {message:?}, properties: {properties:?})",
			);
			return Outcome::Error { error: XcmError::Barrier }
		}

		*id = properties.message_id.unwrap_or(*id);

		let mut vm = Self::new(origin, *id);

		while !message.0.is_empty() {
			let result = vm.process(message);
			log::trace!(target: "xcm::execute", "result: {result:?}");
			message = if let Err(error) = result {
				vm.total_surplus.saturating_accrue(error.weight);
				vm.error = Some((error.index, error.xcm_error));
				vm.take_error_handler().or_else(|| vm.take_appendix())
			} else {
				vm.drop_error_handler();
				vm.take_appendix()
			}
		}

		vm.post_process(xcm_weight)
	}

	fn charge_fees(origin: impl Into<Location>, fees: Assets) -> XcmResult {
		let origin = origin.into();
		if !Config::FeeManager::is_waived(Some(&origin), FeeReason::ChargeFees) {
			for asset in fees.inner() {
				Config::AssetTransactor::withdraw_asset(&asset, &origin, None)?;
			}
			Config::FeeManager::handle_fee(fees, None, FeeReason::ChargeFees);
		}
		Ok(())
	}
}

impl<Config: config::Config> XcmAssetTransfers for XcmExecutor<Config> {
	type IsReserve = Config::IsReserve;
	type IsTeleporter = Config::IsTeleporter;
	type AssetTransactor = Config::AssetTransactor;
}

#[derive(Debug)]
pub struct ExecutorError {
	pub index: u32,
	pub xcm_error: XcmError,
	pub weight: Weight,
}

#[cfg(feature = "runtime-benchmarks")]
impl From<ExecutorError> for frame_benchmarking::BenchmarkError {
	fn from(error: ExecutorError) -> Self {
		log::error!(
			"XCM ERROR >> Index: {:?}, Error: {:?}, Weight: {:?}",
			error.index,
			error.xcm_error,
			error.weight
		);
		Self::Stop("xcm executor error: see error logs")
	}
}

impl<Config: config::Config> XcmExecutor<Config> {
	pub fn new(origin: impl Into<Location>, message_id: XcmHash) -> Self {
		let origin = origin.into();
		Self {
			holding: AssetsInHolding::new(),
			holding_limit: Config::MaxAssetsIntoHolding::get() as usize,
			context: XcmContext { origin: Some(origin.clone()), message_id, topic: None },
			original_origin: origin,
			trader: Config::Trader::new(),
			error: None,
			total_surplus: Weight::zero(),
			total_refunded: Weight::zero(),
			error_handler: Xcm(vec![]),
			error_handler_weight: Weight::zero(),
			appendix: Xcm(vec![]),
			appendix_weight: Weight::zero(),
			transact_status: Default::default(),
			fees_mode: FeesMode { jit_withdraw: false },
			_config: PhantomData,
		}
	}

	/// Execute any final operations after having executed the XCM message.
	/// This includes refunding surplus weight, trapping extra holding funds, and returning any
	/// errors during execution.
	pub fn post_process(mut self, xcm_weight: Weight) -> Outcome {
		// We silently drop any error from our attempt to refund the surplus as it's a charitable
		// thing so best-effort is all we will do.
		let _ = self.refund_surplus();
		drop(self.trader);

		let mut weight_used = xcm_weight.saturating_sub(self.total_surplus);

		if !self.holding.is_empty() {
			log::trace!(
				target: "xcm::post_process",
				"Trapping assets in holding register: {:?}, context: {:?} (original_origin: {:?})",
				self.holding, self.context, self.original_origin,
			);
			let effective_origin = self.context.origin.as_ref().unwrap_or(&self.original_origin);
			let trap_weight =
				Config::AssetTrap::drop_assets(effective_origin, self.holding, &self.context);
			weight_used.saturating_accrue(trap_weight);
		};

		match self.error {
			None => Outcome::Complete { used: weight_used },
			// TODO: #2841 #REALWEIGHT We should deduct the cost of any instructions following
			// the error which didn't end up being executed.
			Some((_i, e)) => {
				log::trace!(target: "xcm::post_process", "Execution errored at {:?}: {:?} (original_origin: {:?})", _i, e, self.original_origin);
				Outcome::Incomplete { used: weight_used, error: e }
			},
		}
	}

	fn origin_ref(&self) -> Option<&Location> {
		self.context.origin.as_ref()
	}

	fn cloned_origin(&self) -> Option<Location> {
		self.context.origin.clone()
	}

	/// Send an XCM, charging fees from Holding as needed.
	fn send(
		&mut self,
		dest: Location,
		msg: Xcm<()>,
		reason: FeeReason,
	) -> Result<XcmHash, XcmError> {
		let (ticket, fee) = validate_send::<Config::XcmSender>(dest, msg)?;
		self.take_fee(fee, reason)?;
		Config::XcmSender::deliver(ticket).map_err(Into::into)
	}

	/// Remove the registered error handler and return it. Do not refund its weight.
	fn take_error_handler(&mut self) -> Xcm<Config::RuntimeCall> {
		let mut r = Xcm::<Config::RuntimeCall>(vec![]);
		sp_std::mem::swap(&mut self.error_handler, &mut r);
		self.error_handler_weight = Weight::zero();
		r
	}

	/// Drop the registered error handler and refund its weight.
	fn drop_error_handler(&mut self) {
		self.error_handler = Xcm::<Config::RuntimeCall>(vec![]);
		self.total_surplus.saturating_accrue(self.error_handler_weight);
		self.error_handler_weight = Weight::zero();
	}

	/// Remove the registered appendix and return it.
	fn take_appendix(&mut self) -> Xcm<Config::RuntimeCall> {
		let mut r = Xcm::<Config::RuntimeCall>(vec![]);
		sp_std::mem::swap(&mut self.appendix, &mut r);
		self.appendix_weight = Weight::zero();
		r
	}

	fn ensure_can_subsume_assets(&self, assets_length: usize) -> Result<(), XcmError> {
		// worst-case, holding.len becomes 2 * holding_limit.
		// this guarantees that if holding.len() == holding_limit and you have more than
		// `holding_limit` items (which has a best case outcome of holding.len() == holding_limit),
		// then the operation is guaranteed to succeed.
		let worst_case_holding_len = self.holding.len() + assets_length;
		log::trace!(target: "xcm::ensure_can_subsume_assets", "worst_case_holding_len: {:?}, holding_limit: {:?}", worst_case_holding_len, self.holding_limit);
		ensure!(worst_case_holding_len <= self.holding_limit * 2, XcmError::HoldingWouldOverflow);
		Ok(())
	}

	/// Refund any unused weight.
	fn refund_surplus(&mut self) -> Result<(), XcmError> {
		let current_surplus = self.total_surplus.saturating_sub(self.total_refunded);
		log::trace!(
			target: "xcm::refund_surplus",
			"total_surplus: {:?}, total_refunded: {:?}, current_surplus: {:?}",
			self.total_surplus,
			self.total_refunded,
			current_surplus,
		);
		if current_surplus.any_gt(Weight::zero()) {
			if let Some(w) = self.trader.refund_weight(current_surplus, &self.context) {
				if !self.holding.contains_asset(&(w.id.clone(), 1).into()) &&
					self.ensure_can_subsume_assets(1).is_err()
				{
					let _ = self
						.trader
						.buy_weight(current_surplus, w.into(), &self.context)
						.defensive_proof(
							"refund_weight returned an asset capable of buying weight; qed",
						);
					log::error!(
						target: "xcm::refund_surplus",
						"error: HoldingWouldOverflow",
					);
					return Err(XcmError::HoldingWouldOverflow)
				}
				self.total_refunded.saturating_accrue(current_surplus);
				self.holding.subsume_assets(w.into());
			}
		}
		log::trace!(
			target: "xcm::refund_surplus",
			"total_refunded: {:?}",
			self.total_refunded,
		);
		Ok(())
	}

	fn take_fee(&mut self, fee: Assets, reason: FeeReason) -> XcmResult {
		if Config::FeeManager::is_waived(self.origin_ref(), reason.clone()) {
			return Ok(())
		}
		log::trace!(
			target: "xcm::fees",
			"taking fee: {:?} from origin_ref: {:?} in fees_mode: {:?} for a reason: {:?}",
			fee,
			self.origin_ref(),
			self.fees_mode,
			reason,
		);
		let paid = if self.fees_mode.jit_withdraw {
			let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
			for asset in fee.inner() {
				Config::AssetTransactor::withdraw_asset(&asset, origin, Some(&self.context))?;
			}
			fee
		} else {
			self.holding.try_take(fee.into()).map_err(|_| XcmError::NotHoldingFees)?.into()
		};
		Config::FeeManager::handle_fee(paid, Some(&self.context), reason);
		Ok(())
	}

	/// Calculates what `local_querier` would be from the perspective of `destination`.
	fn to_querier(
		local_querier: Option<Location>,
		destination: &Location,
	) -> Result<Option<Location>, XcmError> {
		Ok(match local_querier {
			None => None,
			Some(q) => Some(
				q.reanchored(&destination, &Config::UniversalLocation::get())
					.map_err(|_| XcmError::ReanchorFailed)?,
			),
		})
	}

	/// Send a bare `QueryResponse` message containing `response` informed by the given `info`.
	///
	/// The `local_querier` argument is the querier (if any) specified from the *local* perspective.
	fn respond(
		&mut self,
		local_querier: Option<Location>,
		response: Response,
		info: QueryResponseInfo,
		fee_reason: FeeReason,
	) -> Result<XcmHash, XcmError> {
		let querier = Self::to_querier(local_querier, &info.destination)?;
		let QueryResponseInfo { destination, query_id, max_weight } = info;
		let instruction = QueryResponse { query_id, response, max_weight, querier };
		let message = Xcm(vec![instruction]);
		self.send(destination, message, fee_reason)
	}

	fn try_reanchor<T: Reanchorable>(
		reanchorable: T,
		destination: &Location,
	) -> Result<(T, InteriorLocation), XcmError> {
		let reanchor_context = Config::UniversalLocation::get();
		let reanchored =
			reanchorable.reanchored(&destination, &reanchor_context).map_err(|error| {
				log::error!(target: "xcm::reanchor", "Failed reanchoring with error {error:?}");
				XcmError::ReanchorFailed
			})?;
		Ok((reanchored, reanchor_context))
	}

	/// NOTE: Any assets which were unable to be reanchored are introduced into `failed_bin`.
	fn reanchored(
		mut assets: AssetsInHolding,
		dest: &Location,
		maybe_failed_bin: Option<&mut AssetsInHolding>,
	) -> Assets {
		let reanchor_context = Config::UniversalLocation::get();
		assets.reanchor(dest, &reanchor_context, maybe_failed_bin);
		assets.into_assets_iter().collect::<Vec<_>>().into()
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn bench_process(&mut self, xcm: Xcm<Config::RuntimeCall>) -> Result<(), ExecutorError> {
		self.process(xcm)
	}

	fn process(&mut self, xcm: Xcm<Config::RuntimeCall>) -> Result<(), ExecutorError> {
		log::trace!(
			target: "xcm::process",
			"origin: {:?}, total_surplus/refunded: {:?}/{:?}, error_handler_weight: {:?}",
			self.origin_ref(),
			self.total_surplus,
			self.total_refunded,
			self.error_handler_weight,
		);
		let mut result = Ok(());
		for (i, instr) in xcm.0.into_iter().enumerate() {
			match &mut result {
				r @ Ok(()) => {
					// Initialize the recursion count only the first time we hit this code in our
					// potential recursive execution.
					let inst_res = recursion_count::using_once(&mut 1, || {
						recursion_count::with(|count| {
							if *count > RECURSION_LIMIT {
								return Err(XcmError::ExceedsStackLimit)
							}
							*count = count.saturating_add(1);
							Ok(())
						})
						// This should always return `Some`, but let's play it safe.
						.unwrap_or(Ok(()))?;

						// Ensure that we always decrement the counter whenever we finish processing
						// the instruction.
						defer! {
							recursion_count::with(|count| {
								*count = count.saturating_sub(1);
							});
						}

						self.process_instruction(instr)
					});
					if let Err(e) = inst_res {
						log::trace!(target: "xcm::execute", "!!! ERROR: {:?}", e);
						*r = Err(ExecutorError {
							index: i as u32,
							xcm_error: e,
							weight: Weight::zero(),
						});
					}
				},
				Err(ref mut error) =>
					if let Ok(x) = Config::Weigher::instr_weight(&instr) {
						error.weight.saturating_accrue(x)
					},
			}
		}
		result
	}

	/// Process a single XCM instruction, mutating the state of the XCM virtual machine.
	fn process_instruction(
		&mut self,
		instr: Instruction<Config::RuntimeCall>,
	) -> Result<(), XcmError> {
		log::trace!(
			target: "xcm::process_instruction",
			"=== {:?}",
			instr
		);
		match instr {
			WithdrawAsset(assets) => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				self.ensure_can_subsume_assets(assets.len())?;
				Config::TransactionalProcessor::process(|| {
					// Take `assets` from the origin account (on-chain)...
					for asset in assets.inner() {
						Config::AssetTransactor::withdraw_asset(
							asset,
							origin,
							Some(&self.context),
						)?;
					}
					Ok(())
				})
				.and_then(|_| {
					// ...and place into holding.
					self.holding.subsume_assets(assets.into());
					Ok(())
				})
			},
			ReserveAssetDeposited(assets) => {
				// check whether we trust origin to be our reserve location for this asset.
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				self.ensure_can_subsume_assets(assets.len())?;
				for asset in assets.inner() {
					// Must ensure that we recognise the asset as being managed by the origin.
					ensure!(
						Config::IsReserve::contains(asset, origin),
						XcmError::UntrustedReserveLocation
					);
				}
				self.holding.subsume_assets(assets.into());
				Ok(())
			},
			TransferAsset { assets, beneficiary } => {
				Config::TransactionalProcessor::process(|| {
					// Take `assets` from the origin account (on-chain) and place into dest account.
					let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
					for asset in assets.inner() {
						Config::AssetTransactor::transfer_asset(
							&asset,
							origin,
							&beneficiary,
							&self.context,
						)?;
					}
					Ok(())
				})
			},
			TransferReserveAsset { mut assets, dest, xcm } => {
				Config::TransactionalProcessor::process(|| {
					let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
					// Take `assets` from the origin account (on-chain) and place into dest account.
					for asset in assets.inner() {
						Config::AssetTransactor::transfer_asset(
							asset,
							origin,
							&dest,
							&self.context,
						)?;
					}
					let reanchor_context = Config::UniversalLocation::get();
					assets
						.reanchor(&dest, &reanchor_context)
						.map_err(|()| XcmError::LocationFull)?;
					let mut message = vec![ReserveAssetDeposited(assets), ClearOrigin];
					message.extend(xcm.0.into_iter());
					self.send(dest, Xcm(message), FeeReason::TransferReserveAsset)?;
					Ok(())
				})
			},
			ReceiveTeleportedAsset(assets) => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				self.ensure_can_subsume_assets(assets.len())?;
				Config::TransactionalProcessor::process(|| {
					// check whether we trust origin to teleport this asset to us via config trait.
					for asset in assets.inner() {
						// We only trust the origin to send us assets that they identify as their
						// sovereign assets.
						ensure!(
							Config::IsTeleporter::contains(asset, origin),
							XcmError::UntrustedTeleportLocation
						);
						// We should check that the asset can actually be teleported in (for this to
						// be in error, there would need to be an accounting violation by one of the
						// trusted chains, so it's unlikely, but we don't want to punish a possibly
						// innocent chain/user).
						Config::AssetTransactor::can_check_in(origin, asset, &self.context)?;
						Config::AssetTransactor::check_in(origin, asset, &self.context);
					}
					Ok(())
				})
				.and_then(|_| {
					self.holding.subsume_assets(assets.into());
					Ok(())
				})
			},
			Transact { origin_kind, require_weight_at_most, mut call } => {
				// We assume that the Relay-chain is allowed to use transact on this parachain.
				let origin = self.cloned_origin().ok_or_else(|| {
					log::trace!(
						target: "xcm::process_instruction::transact",
						"No origin provided",
					);

					XcmError::BadOrigin
				})?;

				// TODO: #2841 #TRANSACTFILTER allow the trait to issue filters for the relay-chain
				let message_call = call.take_decoded().map_err(|_| {
					log::trace!(
						target: "xcm::process_instruction::transact",
						"Failed to decode call",
					);

					XcmError::FailedToDecode
				})?;

				log::trace!(
					target: "xcm::process_instruction::transact",
					"Processing call: {message_call:?}",
				);

				if !Config::SafeCallFilter::contains(&message_call) {
					log::trace!(
						target: "xcm::process_instruction::transact",
						"Call filtered by `SafeCallFilter`",
					);

					return Err(XcmError::NoPermission)
				}

				let dispatch_origin =
					Config::OriginConverter::convert_origin(origin.clone(), origin_kind).map_err(
						|_| {
							log::trace!(
								target: "xcm::process_instruction::transact",
								"Failed to convert origin {origin:?} and origin kind {origin_kind:?} to a local origin."
							);

							XcmError::BadOrigin
						},
					)?;

				log::trace!(
					target: "xcm::process_instruction::transact",
					"Dispatching with origin: {dispatch_origin:?}",
				);

				let weight = message_call.get_dispatch_info().weight;

				if !weight.all_lte(require_weight_at_most) {
					log::trace!(
						target: "xcm::process_instruction::transact",
						"Max {weight} bigger than require at most {require_weight_at_most}",
					);

					return Err(XcmError::MaxWeightInvalid)
				}

				let maybe_actual_weight =
					match Config::CallDispatcher::dispatch(message_call, dispatch_origin) {
						Ok(post_info) => {
							log::trace!(
								target: "xcm::process_instruction::transact",
								"Dispatch successful: {post_info:?}"
							);
							self.transact_status = MaybeErrorCode::Success;
							post_info.actual_weight
						},
						Err(error_and_info) => {
							log::trace!(
								target: "xcm::process_instruction::transact",
								"Dispatch failed {error_and_info:?}"
							);

							self.transact_status = error_and_info.error.encode().into();
							error_and_info.post_info.actual_weight
						},
					};
				let actual_weight = maybe_actual_weight.unwrap_or(weight);
				let surplus = weight.saturating_sub(actual_weight);
				// We assume that the `Config::Weigher` will counts the `require_weight_at_most`
				// for the estimate of how much weight this instruction will take. Now that we know
				// that it's less, we credit it.
				//
				// We make the adjustment for the total surplus, which is used eventually
				// reported back to the caller and this ensures that they account for the total
				// weight consumed correctly (potentially allowing them to do more operations in a
				// block than they otherwise would).
				self.total_surplus.saturating_accrue(surplus);
				Ok(())
			},
			QueryResponse { query_id, response, max_weight, querier } => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				Config::ResponseHandler::on_response(
					origin,
					query_id,
					querier.as_ref(),
					response,
					max_weight,
					&self.context,
				);
				Ok(())
			},
			DescendOrigin(who) => self
				.context
				.origin
				.as_mut()
				.ok_or(XcmError::BadOrigin)?
				.append_with(who)
				.map_err(|_| XcmError::LocationFull),
			ClearOrigin => {
				self.context.origin = None;
				Ok(())
			},
			ReportError(response_info) => {
				// Report the given result by sending a QueryResponse XCM to a previously given
				// outcome destination if one was registered.
				self.respond(
					self.cloned_origin(),
					Response::ExecutionResult(self.error),
					response_info,
					FeeReason::Report,
				)?;
				Ok(())
			},
			DepositAsset { assets, beneficiary } => {
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					let deposited = self.holding.saturating_take(assets);
					for asset in deposited.into_assets_iter() {
						Config::AssetTransactor::deposit_asset(
							&asset,
							&beneficiary,
							Some(&self.context),
						)?;
					}
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			DepositReserveAsset { assets, dest, xcm } => {
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					// we need to do this take/put cycle to solve wildcards and get exact assets to
					// be weighed
					let to_weigh = self.holding.saturating_take(assets.clone());
					self.holding.subsume_assets(to_weigh.clone());

					let mut message_to_weigh =
						vec![ReserveAssetDeposited(to_weigh.into()), ClearOrigin];
					message_to_weigh.extend(xcm.0.clone().into_iter());
					let (_, fee) =
						validate_send::<Config::XcmSender>(dest.clone(), Xcm(message_to_weigh))?;
					// set aside fee to be charged by XcmSender
					let transport_fee = self.holding.saturating_take(fee.into());

					// now take assets to deposit (excluding transport_fee)
					let deposited = self.holding.saturating_take(assets);
					for asset in deposited.assets_iter() {
						Config::AssetTransactor::deposit_asset(&asset, &dest, Some(&self.context))?;
					}
					// Note that we pass `None` as `maybe_failed_bin` and drop any assets which
					// cannot be reanchored  because we have already called `deposit_asset` on all
					// assets.
					let assets = Self::reanchored(deposited, &dest, None);
					let mut message = vec![ReserveAssetDeposited(assets), ClearOrigin];
					message.extend(xcm.0.into_iter());
					// put back transport_fee in holding register to be charged by XcmSender
					self.holding.subsume_assets(transport_fee);
					self.send(dest, Xcm(message), FeeReason::DepositReserveAsset)?;
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			InitiateReserveWithdraw { assets, reserve, xcm } => {
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					// Note that here we are able to place any assets which could not be reanchored
					// back into Holding.
					let assets = Self::reanchored(
						self.holding.saturating_take(assets),
						&reserve,
						Some(&mut self.holding),
					);
					let mut message = vec![WithdrawAsset(assets), ClearOrigin];
					message.extend(xcm.0.into_iter());
					self.send(reserve, Xcm(message), FeeReason::InitiateReserveWithdraw)?;
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			InitiateTeleport { assets, dest, xcm } => {
				let old_holding = self.holding.clone();
				let result = (|| -> Result<(), XcmError> {
					// We must do this first in order to resolve wildcards.
					let assets = self.holding.saturating_take(assets);
					for asset in assets.assets_iter() {
						// We should check that the asset can actually be teleported out (for this
						// to be in error, there would need to be an accounting violation by
						// ourselves, so it's unlikely, but we don't want to allow that kind of bug
						// to leak into a trusted chain.
						Config::AssetTransactor::can_check_out(&dest, &asset, &self.context)?;
					}
					// Note that we pass `None` as `maybe_failed_bin` and drop any assets which
					// cannot be reanchored  because we have already checked all assets out.
					let reanchored_assets = Self::reanchored(assets.clone(), &dest, None);
					let mut message = vec![ReceiveTeleportedAsset(reanchored_assets), ClearOrigin];
					message.extend(xcm.0.into_iter());
					self.send(dest.clone(), Xcm(message), FeeReason::InitiateTeleport)?;

					for asset in assets.assets_iter() {
						Config::AssetTransactor::check_out(&dest, &asset, &self.context);
					}
					Ok(())
				})();
				if result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			ReportHolding { response_info, assets } => {
				// Note that we pass `None` as `maybe_failed_bin` since no assets were ever removed
				// from Holding.
				let assets =
					Self::reanchored(self.holding.min(&assets), &response_info.destination, None);
				self.respond(
					self.cloned_origin(),
					Response::Assets(assets),
					response_info,
					FeeReason::Report,
				)?;
				Ok(())
			},
			BuyExecution { fees, weight_limit } => {
				// There is no need to buy any weight if `weight_limit` is `Unlimited` since it
				// would indicate that `AllowTopLevelPaidExecutionFrom` was unused for execution
				// and thus there is some other reason why it has been determined that this XCM
				// should be executed.
				let Some(weight) = Option::<Weight>::from(weight_limit) else { return Ok(()) };
				let old_holding = self.holding.clone();
				// pay for `weight` using up to `fees` of the holding register.
				let max_fee =
					self.holding.try_take(fees.into()).map_err(|_| XcmError::NotHoldingFees)?;
				let result = || -> Result<(), XcmError> {
					let unspent = self.trader.buy_weight(weight, max_fee, &self.context)?;
					self.holding.subsume_assets(unspent);
					Ok(())
				}();
				if result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			RefundSurplus => self.refund_surplus(),
			SetErrorHandler(mut handler) => {
				let handler_weight = Config::Weigher::weight(&mut handler)
					.map_err(|()| XcmError::WeightNotComputable)?;
				self.total_surplus.saturating_accrue(self.error_handler_weight);
				self.error_handler = handler;
				self.error_handler_weight = handler_weight;
				Ok(())
			},
			SetAppendix(mut appendix) => {
				let appendix_weight = Config::Weigher::weight(&mut appendix)
					.map_err(|()| XcmError::WeightNotComputable)?;
				self.total_surplus.saturating_accrue(self.appendix_weight);
				self.appendix = appendix;
				self.appendix_weight = appendix_weight;
				Ok(())
			},
			ClearError => {
				self.error = None;
				Ok(())
			},
			ClaimAsset { assets, ticket } => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				self.ensure_can_subsume_assets(assets.len())?;
				let ok = Config::AssetClaims::claim_assets(origin, &ticket, &assets, &self.context);
				ensure!(ok, XcmError::UnknownClaim);
				self.holding.subsume_assets(assets.into());
				Ok(())
			},
			Trap(code) => Err(XcmError::Trap(code)),
			SubscribeVersion { query_id, max_response_weight } => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				// We don't allow derivative origins to subscribe since it would otherwise pose a
				// DoS risk.
				ensure!(&self.original_origin == origin, XcmError::BadOrigin);
				Config::SubscriptionService::start(
					origin,
					query_id,
					max_response_weight,
					&self.context,
				)
			},
			UnsubscribeVersion => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				ensure!(&self.original_origin == origin, XcmError::BadOrigin);
				Config::SubscriptionService::stop(origin, &self.context)
			},
			BurnAsset(assets) => {
				self.holding.saturating_take(assets.into());
				Ok(())
			},
			ExpectAsset(assets) =>
				self.holding.ensure_contains(&assets).map_err(|_| XcmError::ExpectationFalse),
			ExpectOrigin(origin) => {
				ensure!(self.context.origin == origin, XcmError::ExpectationFalse);
				Ok(())
			},
			ExpectError(error) => {
				ensure!(self.error == error, XcmError::ExpectationFalse);
				Ok(())
			},
			ExpectTransactStatus(transact_status) => {
				ensure!(self.transact_status == transact_status, XcmError::ExpectationFalse);
				Ok(())
			},
			QueryPallet { module_name, response_info } => {
				let pallets = Config::PalletInstancesInfo::infos()
					.into_iter()
					.filter(|x| x.module_name.as_bytes() == &module_name[..])
					.map(|x| {
						PalletInfo::new(
							x.index as u32,
							x.name.as_bytes().into(),
							x.module_name.as_bytes().into(),
							x.crate_version.major as u32,
							x.crate_version.minor as u32,
							x.crate_version.patch as u32,
						)
					})
					.collect::<Result<Vec<_>, XcmError>>()?;
				let QueryResponseInfo { destination, query_id, max_weight } = response_info;
				let response =
					Response::PalletsInfo(pallets.try_into().map_err(|_| XcmError::Overflow)?);
				let querier = Self::to_querier(self.cloned_origin(), &destination)?;
				let instruction = QueryResponse { query_id, response, max_weight, querier };
				let message = Xcm(vec![instruction]);
				self.send(destination, message, FeeReason::QueryPallet)?;
				Ok(())
			},
			ExpectPallet { index, name, module_name, crate_major, min_crate_minor } => {
				let pallet = Config::PalletInstancesInfo::infos()
					.into_iter()
					.find(|x| x.index == index as usize)
					.ok_or(XcmError::PalletNotFound)?;
				ensure!(pallet.name.as_bytes() == &name[..], XcmError::NameMismatch);
				ensure!(pallet.module_name.as_bytes() == &module_name[..], XcmError::NameMismatch);
				let major = pallet.crate_version.major as u32;
				ensure!(major == crate_major, XcmError::VersionIncompatible);
				let minor = pallet.crate_version.minor as u32;
				ensure!(minor >= min_crate_minor, XcmError::VersionIncompatible);
				Ok(())
			},
			ReportTransactStatus(response_info) => {
				self.respond(
					self.cloned_origin(),
					Response::DispatchResult(self.transact_status.clone()),
					response_info,
					FeeReason::Report,
				)?;
				Ok(())
			},
			ClearTransactStatus => {
				self.transact_status = Default::default();
				Ok(())
			},
			UniversalOrigin(new_global) => {
				let universal_location = Config::UniversalLocation::get();
				ensure!(universal_location.first() != Some(&new_global), XcmError::InvalidLocation);
				let origin = self.cloned_origin().ok_or(XcmError::BadOrigin)?;
				let origin_xform = (origin, new_global);
				let ok = Config::UniversalAliases::contains(&origin_xform);
				ensure!(ok, XcmError::InvalidLocation);
				let (_, new_global) = origin_xform;
				let new_origin = Junctions::from([new_global]).relative_to(&universal_location);
				self.context.origin = Some(new_origin);
				Ok(())
			},
			ExportMessage { network, destination, xcm } => {
				// The actual message sent to the bridge for forwarding is prepended with
				// `UniversalOrigin` and `DescendOrigin` in order to ensure that the message is
				// executed with this Origin.
				//
				// Prepend the desired message with instructions which effectively rewrite the
				// origin.
				//
				// This only works because the remote chain empowers the bridge
				// to speak for the local network.
				let origin = self.context.origin.as_ref().ok_or(XcmError::BadOrigin)?.clone();
				let universal_source = Config::UniversalLocation::get()
					.within_global(origin)
					.map_err(|()| XcmError::Unanchored)?;
				let hash = (self.origin_ref(), &destination).using_encoded(blake2_128);
				let channel = u32::decode(&mut hash.as_ref()).unwrap_or(0);
				// Hash identifies the lane on the exporter which we use. We use the pairwise
				// combination of the origin and destination to ensure origin/destination pairs
				// will generally have their own lanes.
				let (ticket, fee) = validate_export::<Config::MessageExporter>(
					network,
					channel,
					universal_source,
					destination.clone(),
					xcm,
				)?;
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					self.take_fee(fee, FeeReason::Export { network, destination })?;
					let _ = Config::MessageExporter::deliver(ticket).defensive_proof(
						"`deliver` called immediately after `validate_export`; \
						`take_fee` does not affect the validity of the ticket; qed",
					);
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			LockAsset { asset, unlocker } => {
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					let origin = self.cloned_origin().ok_or(XcmError::BadOrigin)?;
					let (remote_asset, context) = Self::try_reanchor(asset.clone(), &unlocker)?;
					let lock_ticket =
						Config::AssetLocker::prepare_lock(unlocker.clone(), asset, origin.clone())?;
					let owner = origin
						.reanchored(&unlocker, &context)
						.map_err(|_| XcmError::ReanchorFailed)?;
					let msg = Xcm::<()>(vec![NoteUnlockable { asset: remote_asset, owner }]);
					let (ticket, price) = validate_send::<Config::XcmSender>(unlocker, msg)?;
					self.take_fee(price, FeeReason::LockAsset)?;
					lock_ticket.enact()?;
					Config::XcmSender::deliver(ticket)?;
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			UnlockAsset { asset, target } => {
				let origin = self.cloned_origin().ok_or(XcmError::BadOrigin)?;
				Config::AssetLocker::prepare_unlock(origin, asset, target)?.enact()?;
				Ok(())
			},
			NoteUnlockable { asset, owner } => {
				let origin = self.cloned_origin().ok_or(XcmError::BadOrigin)?;
				Config::AssetLocker::note_unlockable(origin, asset, owner)?;
				Ok(())
			},
			RequestUnlock { asset, locker } => {
				let origin = self.cloned_origin().ok_or(XcmError::BadOrigin)?;
				let remote_asset = Self::try_reanchor(asset.clone(), &locker)?.0;
				let remote_target = Self::try_reanchor(origin.clone(), &locker)?.0;
				let reduce_ticket = Config::AssetLocker::prepare_reduce_unlockable(
					locker.clone(),
					asset,
					origin.clone(),
				)?;
				let msg =
					Xcm::<()>(vec![UnlockAsset { asset: remote_asset, target: remote_target }]);
				let (ticket, price) = validate_send::<Config::XcmSender>(locker, msg)?;
				let old_holding = self.holding.clone();
				let result = Config::TransactionalProcessor::process(|| {
					self.take_fee(price, FeeReason::RequestUnlock)?;
					reduce_ticket.enact()?;
					Config::XcmSender::deliver(ticket)?;
					Ok(())
				});
				if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			ExchangeAsset { give, want, maximal } => {
				let old_holding = self.holding.clone();
				let give = self.holding.saturating_take(give);
				let result = (|| -> Result<(), XcmError> {
					self.ensure_can_subsume_assets(want.len())?;
					let exchange_result = Config::AssetExchanger::exchange_asset(
						self.origin_ref(),
						give,
						&want,
						maximal,
					);
					if let Ok(received) = exchange_result {
						self.holding.subsume_assets(received.into());
						Ok(())
					} else {
						Err(XcmError::NoDeal)
					}
				})();
				if result.is_err() {
					self.holding = old_holding;
				}
				result
			},
			SetFeesMode { jit_withdraw } => {
				self.fees_mode = FeesMode { jit_withdraw };
				Ok(())
			},
			SetTopic(topic) => {
				self.context.topic = Some(topic);
				Ok(())
			},
			ClearTopic => {
				self.context.topic = None;
				Ok(())
			},
			AliasOrigin(target) => {
				let origin = self.origin_ref().ok_or(XcmError::BadOrigin)?;
				if Config::Aliasers::contains(origin, &target) {
					self.context.origin = Some(target);
					Ok(())
				} else {
					Err(XcmError::NoPermission)
				}
			},
			UnpaidExecution { check_origin, .. } => {
				ensure!(
					check_origin.is_none() || self.context.origin == check_origin,
					XcmError::BadOrigin
				);
				Ok(())
			},
			HrmpNewChannelOpenRequest { .. } => Err(XcmError::Unimplemented),
			HrmpChannelAccepted { .. } => Err(XcmError::Unimplemented),
			HrmpChannelClosing { .. } => Err(XcmError::Unimplemented),
		}
	}
}
