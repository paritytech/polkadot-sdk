// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
	statement::{
		api::StatementSpecApiServer,
		error::Error,
		event::{AddFilterResponse, SubmitOutcome},
		subscription::{
			add_filter_sync, parse_filter_id, remove_filter_sync, run_subscription_task,
			AddFilterOutcome, StatementSubscriptions,
		},
		LOG_TARGET,
	},
	SubscriptionTaskExecutor,
};
use codec::Decode;
use futures::FutureExt;
use jsonrpsee::{
	core::async_trait, types::SubscriptionId, ConnectionId, Extensions, PendingSubscriptionSink,
};
use sc_rpc::utils::Subscription;
use sc_statement_store::MultiFilterSubscriptionApi;
use sp_core::Bytes;
use sp_statement_store::{
	OptimizedTopicFilter, Statement, StatementSource, StatementStore, SubmitResult, TopicFilter,
};
use std::sync::Arc;

/// JSON-RPC server implementation for the `statement_unstable_*` methods
pub struct StatementSpec<B> {
	store: Arc<B>,
	executor: SubscriptionTaskExecutor,
	subscriptions: StatementSubscriptions,
}

impl<B> StatementSpec<B>
where
	B: StatementStore + Send + Sync + 'static,
	Arc<B>: MultiFilterSubscriptionApi,
{
	pub fn new(store: Arc<B>, executor: SubscriptionTaskExecutor) -> Self {
		Self { store, executor, subscriptions: StatementSubscriptions::new() }
	}
}

fn read_subscription_id_as_string(sink: &Subscription) -> String {
	match sink.subscription_id() {
		SubscriptionId::Num(n) => n.to_string(),
		SubscriptionId::Str(s) => s.into_owned().into(),
	}
}

fn connection_id(ext: &Extensions) -> ConnectionId {
	ext.get::<ConnectionId>()
		.copied()
		.expect("ConnectionId is always set by jsonrpsee; qed")
}

fn validate_topic_filter(filter: TopicFilter) -> Result<OptimizedTopicFilter, Error> {
	match &filter {
		TopicFilter::MatchAny(_) => Err(Error::InvalidParam(
			"`matchAny` topic filter is not supported by statement_unstable_add_filter; \
			 use `\"any\"` or `{\"matchAll\": [...]}` instead"
				.to_string(),
		)),
		_ => Ok(filter.into()),
	}
}

#[async_trait]
impl<B> StatementSpecApiServer for StatementSpec<B>
where
	B: StatementStore + Send + Sync + 'static,
	Arc<B>: MultiFilterSubscriptionApi,
{
	fn statement_unstable_subscribe(&self, pending: PendingSubscriptionSink, _ext: &Extensions) {
		let subscriptions = self.subscriptions.clone();
		let store = self.store.clone();
		let executor = self.executor.clone();

		let fut = async move {
			let connection_id = pending.connection_id();
			let Some(reserved) = subscriptions.reserve(connection_id) else {
				pending.reject(Error::ReachedLimits).await;
				return;
			};

			let Ok(sink) = pending.accept().await.map(Subscription::from) else { return };
			let sub_id = read_subscription_id_as_string(&sink);

			let (handle, live_stream) = store.create_subscription();
			let Some(entry) = reserved.register(sub_id.clone(), handle) else {
				log::debug!(target: LOG_TARGET, "duplicate subscription id {sub_id}; aborting");
				return;
			};

			let task = run_subscription_task(sink, live_stream);
			executor.spawn(
				"statement-unstable-subscribe",
				Some("rpc"),
				async move {
					task.await;
					drop(entry);
				}
				.boxed(),
			);
		};

		self.executor
			.spawn("statement-unstable-subscribe-init", Some("rpc"), fut.boxed());
	}

	async fn statement_unstable_add_filter(
		&self,
		ext: &Extensions,
		subscription: String,
		topic_filter: TopicFilter,
	) -> Result<AddFilterResponse, Error> {
		let conn_id = connection_id(ext);
		let topic_filter = validate_topic_filter(topic_filter)?;

		let Some(state) = self.subscriptions.get(conn_id, &subscription) else {
			return Err(Error::InvalidSubscription);
		};

		match add_filter_sync(&state, topic_filter)? {
			AddFilterOutcome::Added(filter_id) => Ok(AddFilterResponse::Ok(
				crate::statement::subscription::filter_id_to_string(filter_id),
			)),
			AddFilterOutcome::LimitReached => Ok(AddFilterResponse::limit_reached()),
		}
	}

	async fn statement_unstable_remove_filter(
		&self,
		ext: &Extensions,
		subscription: String,
		filter_id: String,
	) -> Result<(), Error> {
		let conn_id = connection_id(ext);
		let Some(state) = self.subscriptions.get(conn_id, &subscription) else { return Ok(()) };
		let Some(parsed) = parse_filter_id(&filter_id) else { return Ok(()) };
		let _ = remove_filter_sync(&state, parsed);
		Ok(())
	}

	async fn statement_unstable_submit(&self, encoded: Bytes) -> Result<SubmitOutcome, Error> {
		let statement = Statement::decode(&mut &encoded[..])
			.map_err(|e| Error::InvalidParam(format!("Error decoding statement: {e}")))?;
		if self.store.has_statement(&statement.hash()) {
			return Ok(SubmitOutcome::Known);
		}
		match self.store.submit(statement, StatementSource::Local) {
			SubmitResult::New => Ok(SubmitOutcome::New),
			SubmitResult::Known => Ok(SubmitOutcome::Known),
			SubmitResult::Rejected(reason) => Ok(SubmitOutcome::Rejected(reason)),
			SubmitResult::Invalid(reason) => Ok(SubmitOutcome::Invalid(reason)),
			SubmitResult::KnownExpired => {
				Err(Error::InternalError("store returned KnownExpired for local submit".into()))
			},
			SubmitResult::InternalError(e) => Err(Error::InternalError(e.to_string())),
		}
	}
}
