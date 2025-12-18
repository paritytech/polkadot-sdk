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

//! Subscription logic for statement store.
//!
//! Manages subscriptions to statement topics and notifies subscribers when new statements arrive.
//! Uses multiple matcher tasks to handle subscriptions concurrently, each responsible for a subset
//! of subscriptions. Each matcher task maintains its own list of subscriptions and matches incoming
//! statements against them. When a new statement is submitted, it is sent to all matcher tasks for
//! processing. If a statement matches a subscription's filter, it is sent to the subscriber via an
//! async channel.
//!
//! This design allows for efficient handling of a large number of subscriptions and statements and
//! can be scaled by adjusting the number of matcher tasks.

// Buffer size for the matcher task channels, to backpressure the submission senders.
// This value is generous to allow for bursts of statements without dropping any or backpressuring
// too early.
const MATCHERS_TASK_CHANNEL_BUFFER_SIZE: usize = 80_000;

use futures::{Stream, StreamExt};

use crate::LOG_TARGET;
use sc_utils::id_sequence::SeqID;
use sp_core::{traits::SpawnNamed, Bytes, Encode};
pub use sp_statement_store::StatementStore;
use sp_statement_store::{CheckedTopicFilter, Result, Statement, Topic};
use std::{
	collections::{HashMap, HashSet},
	sync::atomic::AtomicU64,
};

/// Trait for initiating statement store subscriptions from the RPC module.
pub trait StatementStoreSubscriptionApi: Send + Sync {
	/// Subscribe to statements matching the topic filter.
	///
	/// Returns existing matching statements, a sender channel to send matched statements and a
	/// stream for receiving matched statements when they arrive.
	fn subscribe_statement(
		&self,
		topic_filter: CheckedTopicFilter,
	) -> Result<(Vec<Vec<u8>>, async_channel::Sender<Bytes>, SubscriptionStatementsStream)>;
}

#[derive(Clone)]
pub enum MatcherMessage {
	NewStatement(Statement),
	Subscribe(SubscriptionInfo),
	Unsubscribe(SeqID),
}

// Manages subscriptions to statement topics and notifies subscribers when new statements arrive.
pub struct SubscriptionsHandle {
	// Sequence generator for subscription IDs, atomic for thread safety.
	// Subscription creation is expensive enough that we don't worry about overflow here.
	id_sequence: AtomicU64,
	//  Subscriptions matchers handlers.
	matchers: SubscriptionsMatchersHandlers,
}

impl SubscriptionsHandle {
	/// Create a new SubscriptionsHandle with the given task spawner and number of filter workers.
	pub(crate) fn new(
		task_spawner: Box<dyn SpawnNamed>,
		num_matcher_workers: usize,
	) -> SubscriptionsHandle {
		let mut subscriptions_matchers_senders = Vec::with_capacity(num_matcher_workers);

		for _ in 0..num_matcher_workers {
			let (subscription_matcher_sender, subscription_matcher_receiver) =
				async_channel::bounded(MATCHERS_TASK_CHANNEL_BUFFER_SIZE);
			subscriptions_matchers_senders.push(subscription_matcher_sender);
			task_spawner.spawn_blocking(
				"statement-store-subscription-filters",
				Some("statement-store"),
				Box::pin(async move {
					let mut subscriptions = SubscriptionsInfo::new();

					loop {
						match subscription_matcher_receiver.recv().await {
							Ok(MatcherMessage::NewStatement(statement)) => {
								subscriptions.notify_matching_subscribers(&statement);
								subscriptions.notify_any_subscribers(&statement);
							},
							Ok(MatcherMessage::Subscribe(info)) => {
								subscriptions.subscribe(info);
							},
							Ok(MatcherMessage::Unsubscribe(seq_id)) => {
								subscriptions.unsubscribe(seq_id);
							},
							Err(_) => {
								// Expected when the subscription manager is dropped at shutdown.
								log::error!(
									target: LOG_TARGET,
									"Statement subscription matcher channel closed"
								);
								break
							},
						};
					}
				}),
			);
		}
		SubscriptionsHandle {
			id_sequence: AtomicU64::new(0),
			matchers: SubscriptionsMatchersHandlers::new(subscriptions_matchers_senders),
		}
	}

	fn next_id(&self) -> SeqID {
		let id = self.id_sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		SeqID::from(id)
	}

	pub(crate) fn subscribe(
		&self,
		topic_filter: CheckedTopicFilter,
		num_existing_statements: usize,
	) -> (async_channel::Sender<Bytes>, SubscriptionStatementsStream) {
		let next_id = self.next_id();
		let (tx, rx) = async_channel::bounded(std::cmp::max(
			MATCHERS_TASK_CHANNEL_BUFFER_SIZE,
			num_existing_statements,
		));
		let subscription_info =
			SubscriptionInfo { topic_filter: topic_filter.clone(), seq_id: next_id, tx };

		let result = (
			subscription_info.tx.clone(),
			SubscriptionStatementsStream {
				rx,
				sub_id: subscription_info.seq_id,
				matchers: self.matchers.clone(),
			},
		);

		self.matchers
			.send_by_seq_id(subscription_info.seq_id, MatcherMessage::Subscribe(subscription_info));
		result
	}

	pub(crate) fn notify(&self, statement: Statement) {
		self.matchers.send_all(MatcherMessage::NewStatement(statement));
	}
}

// Information about all subscriptions.
// Each matcher task will have its own instance of this struct.
struct SubscriptionsInfo {
	// Subscriptions organized by topic, there can be multiple entries per subscription if it
	// subscribes to multiple topics with MatchAll or MatchAny filters.
	subscriptions_by_topic: HashMap<Topic, HashMap<SeqID, SubscriptionInfo>>,
	// Subscriptions that listen with Any filter (i.e., no topic filtering).
	subscriptions_any: HashMap<SeqID, SubscriptionInfo>,
	// Mapping from subscription ID to topic filter.
	by_sub_id: HashMap<SeqID, CheckedTopicFilter>,
}

// Information about a single subscription.
#[derive(Clone)]
pub(crate) struct SubscriptionInfo {
	// The filter used for this subscription.
	topic_filter: CheckedTopicFilter,
	// The unique ID of this subscription.
	seq_id: SeqID,
	// Channel to send matched statements to the subscriber.
	tx: async_channel::Sender<Bytes>,
}

impl SubscriptionsInfo {
	fn new() -> SubscriptionsInfo {
		SubscriptionsInfo {
			subscriptions_by_topic: HashMap::new(),
			subscriptions_any: HashMap::new(),
			by_sub_id: HashMap::new(),
		}
	}

	// Subscribe a new subscription.
	fn subscribe(&mut self, subscription_info: SubscriptionInfo) {
		self.by_sub_id
			.insert(subscription_info.seq_id, subscription_info.topic_filter.clone());
		let topics = match &subscription_info.topic_filter {
			CheckedTopicFilter::Any => {
				self.subscriptions_any
					.insert(subscription_info.seq_id, subscription_info.clone());
				return;
			},
			CheckedTopicFilter::MatchAll(topics) => topics,
			CheckedTopicFilter::MatchAny(topics) => topics,
		};
		for topic in topics {
			self.subscriptions_by_topic
				.entry(*topic)
				.or_insert_with(Default::default)
				.insert(subscription_info.seq_id, subscription_info.clone());
		}
	}

	// Notify a single subscriber, marking it for unsubscribing if sending fails.
	fn notify_subscriber(
		&self,
		subscription: &SubscriptionInfo,
		bytes_to_send: Bytes,
		needs_unsubscribing: &mut HashSet<SeqID>,
	) {
		if let Err(err) = subscription.tx.try_send(bytes_to_send) {
			log::warn!(
				target: LOG_TARGET,
				"Failed to send statement to subscriber {:?}: {:?} unsubscribing it", subscription.seq_id, err
			);
			// Mark subscription for unsubscribing, to give it a chance to recover the buffers are
			// generous enough, if subscription cannot keep up we unsubscribe it.
			needs_unsubscribing.insert(subscription.seq_id);
		}
	}

	// Notify all subscribers whose filters match the given statement.
	fn notify_matching_subscribers(&mut self, statement: &Statement) {
		// Track how many topics are still needed to match for each subscription.
		// `subscription_by_topic` may contain multiple entries for the same subscription if it
		// subscribes to multiple topics with MatchAll or MatchAny filters.
		// We decrement the counter each time we find a matching topic, and only notify
		// the subscriber when the counter reaches zero.
		let mut matched_senders: HashMap<SeqID, usize> = HashMap::new();
		let bytes_to_send: Bytes = statement.encode().into();
		let mut needs_unsubscribing: HashSet<SeqID> = HashSet::new();

		for statement_topic in statement.topics() {
			if let Some(subscriptions) = self.subscriptions_by_topic.get(statement_topic) {
				for subscription in subscriptions.values() {
					// Check if the statement matches the subscription filter
					if let Some(counter) = matched_senders.get_mut(&subscription.seq_id) {
						if *counter > 0 {
							*counter -= 1;
							if *counter == 0 {
								self.notify_subscriber(
									subscription,
									bytes_to_send.clone(),
									&mut needs_unsubscribing,
								);
							}
						}
					} else {
						match &subscription.topic_filter {
							CheckedTopicFilter::Any => {
								matched_senders.insert(subscription.seq_id, 0);
								self.notify_subscriber(
									subscription,
									bytes_to_send.clone(),
									&mut needs_unsubscribing,
								);
							},
							CheckedTopicFilter::MatchAll(topics) => {
								let counter = topics.len() - 1;

								matched_senders.insert(subscription.seq_id, counter);
								if counter == 0 {
									self.notify_subscriber(
										subscription,
										bytes_to_send.clone(),
										&mut needs_unsubscribing,
									);
								}
							},
							CheckedTopicFilter::MatchAny(_topics) => {
								matched_senders.insert(subscription.seq_id, 0);
								self.notify_subscriber(
									subscription,
									bytes_to_send.clone(),
									&mut needs_unsubscribing,
								);
							},
						}
					}
				}
			}
		}
		// Unsubscribe any subscriptions that failed to receive messages, to give them a chance to
		// recover and not miss statements.
		for sub_id in needs_unsubscribing {
			self.unsubscribe(sub_id);
		}
	}

	// Notify all subscribers that don't filter by topic and want to receive all statements.
	fn notify_any_subscribers(&mut self, statement: &Statement) {
		let mut needs_unsubscribing: HashSet<SeqID> = HashSet::new();

		let bytes_to_send: Bytes = statement.encode().into();
		for subscription in self.subscriptions_any.values() {
			let _ = self.notify_subscriber(
				subscription,
				bytes_to_send.clone(),
				&mut needs_unsubscribing,
			);
		}

		// Unsubscribe any subscriptions that failed to receive messages, to give them a chance to
		// recover and not miss statements.
		for sub_id in needs_unsubscribing {
			self.unsubscribe(sub_id);
		}
	}

	// Unsubscribe a subscription by its ID.
	fn unsubscribe(&mut self, id: SeqID) {
		let entry = match self.by_sub_id.remove(&id) {
			Some(e) => e,
			None => return,
		};

		let topics = match &entry {
			CheckedTopicFilter::Any => {
				self.subscriptions_any.remove(&id);
				return;
			},
			CheckedTopicFilter::MatchAll(topics) => topics,
			CheckedTopicFilter::MatchAny(topics) => topics,
		};

		for topic in topics {
			if let Some(subscriptions) = self.subscriptions_by_topic.get_mut(topic) {
				subscriptions.remove(&id);
				if subscriptions.is_empty() {
					self.subscriptions_by_topic.remove(topic);
				}
			}
		}
	}
}

// Handlers to communicate with subscription matcher tasks.
#[derive(Clone)]
pub struct SubscriptionsMatchersHandlers {
	// Channels to send messages to matcher tasks.
	matchers: Vec<async_channel::Sender<MatcherMessage>>,
}

impl SubscriptionsMatchersHandlers {
	/// Create new SubscriptionsMatchersHandlers with the given matcher task senders.
	fn new(matchers: Vec<async_channel::Sender<MatcherMessage>>) -> SubscriptionsMatchersHandlers {
		SubscriptionsMatchersHandlers { matchers }
	}

	// Send a message to the matcher task responsible for the given subscription ID.
	fn send_by_seq_id(&self, id: SeqID, message: MatcherMessage) {
		let index: u64 = id.into();
		// If matchers channels are full we backpressure the sender, in this case it will be the
		// processing of new statements.
		if let Err(err) = self.matchers[index as usize % self.matchers.len()].send_blocking(message)
		{
			log::error!(
				target: LOG_TARGET,
				"Failed to send statement to matcher task: {:?}", err
			);
		}
	}

	// Send a message to all matcher tasks.
	fn send_all(&self, message: MatcherMessage) {
		for sender in &self.matchers {
			if let Err(err) = sender.send_blocking(message.clone()) {
				log::error!(
					target: LOG_TARGET,
					"Failed to send message to matcher task: {:?}", err
				);
			}
		}
	}
}

// Stream of statements for a subscription.
pub struct SubscriptionStatementsStream {
	// Channel to receive statements.
	pub rx: async_channel::Receiver<Bytes>,
	// Subscription ID, used for cleanup on drop.
	sub_id: SeqID,
	// Reference to the matchers for cleanup.
	matchers: SubscriptionsMatchersHandlers,
}

// When the stream is dropped, unsubscribe from the matchers.
impl Drop for SubscriptionStatementsStream {
	fn drop(&mut self) {
		self.matchers
			.send_by_seq_id(self.sub_id, MatcherMessage::Unsubscribe(self.sub_id));
	}
}

impl Stream for SubscriptionStatementsStream {
	type Item = Bytes;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		self.rx.poll_next_unpin(cx)
	}
}
#[cfg(test)]
mod tests {

	use crate::tests::signed_statement;

	use super::*;
	use sp_core::Decode;
	use sp_statement_store::Topic;
	#[test]
	fn test_subscribe_unsubscribe() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, _rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(vec![topic1, topic2]),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		assert!(subscriptions.subscriptions_by_topic.contains_key(&topic1));
		assert!(subscriptions.subscriptions_by_topic.contains_key(&topic2));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(!subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));

		subscriptions.unsubscribe(sub_info1.seq_id);
		assert!(!subscriptions.subscriptions_by_topic.contains_key(&topic1));
		assert!(!subscriptions.subscriptions_by_topic.contains_key(&topic2));
	}

	#[test]
	fn test_subscribe_any() {
		let mut subscriptions = SubscriptionsInfo::new();
		let (tx1, _rx1) = async_channel::bounded::<Bytes>(10);
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::Any,
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		assert!(subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		subscriptions.unsubscribe(sub_info1.seq_id);
		assert!(!subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));
	}

	#[test]
	fn test_subscribe_match_any() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, _rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(vec![topic1, topic2]),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		assert!(subscriptions.subscriptions_by_topic.contains_key(&topic1));
		assert!(subscriptions.subscriptions_by_topic.contains_key(&topic2));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(!subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));

		subscriptions.unsubscribe(sub_info1.seq_id);
		assert!(!subscriptions.subscriptions_by_topic.contains_key(&topic1));
		assert!(!subscriptions.subscriptions_by_topic.contains_key(&topic2));
	}

	#[test]
	fn test_notify_any_subscribers() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::Any,
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		let mut statement = signed_statement(1);
		subscriptions.notify_any_subscribers(&statement);

		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);
	}

	#[test]
	fn test_notify_match_all_subscribers() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(vec![topic1, topic2]),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic2));
		subscriptions.notify_matching_subscribers(&statement);

		// Should not receive yet, only one topic matched.
		assert!(rx1.try_recv().is_err());

		statement.set_topic(1, Topic::from(topic1));
		subscriptions.notify_matching_subscribers(&statement);

		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);
	}

	#[test]
	fn test_notify_match_any_subscribers() {
		let mut subscriptions = SubscriptionsInfo::new();
		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(vec![topic1, topic2]),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic2));
		subscriptions.notify_matching_subscribers(&statement);
		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);
	}

	#[tokio::test]
	async fn test_subscription_handle_with_different_workers_number() {
		for num_workers in 1..5 {
			let subscriptions_handle = SubscriptionsHandle::new(
				Box::new(sp_core::testing::TaskExecutor::new()),
				num_workers,
			);

			let topic1 = [8u8; 32];
			let topic2 = [9u8; 32];

			let streams = (0..5)
				.into_iter()
				.map(|_| {
					subscriptions_handle
						.subscribe(CheckedTopicFilter::MatchAll(vec![topic1, topic2]))
				})
				.collect::<Vec<_>>();

			let mut statement = signed_statement(1);
			statement.set_topic(0, Topic::from(topic2));
			subscriptions_handle.notify(statement.clone());

			statement.set_topic(1, Topic::from(topic1));
			subscriptions_handle.notify(statement.clone());

			for (tx, mut stream) in streams {
				let received = stream.next().await.expect("Should receive statement");
				let decoded_statement: Statement =
					Statement::decode(&mut &received.0[..]).expect("Should decode statement");
				assert_eq!(decoded_statement, statement);
			}
		}
	}

	// #[tokio::test]
	// async fn test_handle_unsubscribe() {
	// 	let subscriptions_handle =
	// 		SubscriptionsHandle::new(Box::new(sp_core::testing::TaskExecutor::new()), 3);

	// 	let topic1 = [8u8; 32];
	// 	let topic2 = [9u8; 32];

	// 	let streams = (0..5)
	// 		.into_iter()
	// 		.map(|_| subscriptions_handle.subscribe(TopicFilter::MatchAll(vec![topic1, topic2])))
	// 		.collect::<Vec<_>>();

	// 	// Unsubscribe all streams by dropping  SubscriptionStatementsStream
	// 	let rx_channels =
	// 		streams.into_iter().map(|(_, stream)| stream.rx.clone()).collect::<Vec<_>>();

	// 	let mut statement = signed_statement(1);
	// 	statement.set_topic(0, Topic::from(topic2));
	// 	subscriptions_handle.notify(statement.clone());

	// 	statement.set_topic(1, Topic::from(topic1));
	// 	subscriptions_handle.notify(statement.clone());
	// 	for rx in rx_channels {
	// 		assert!(rx.recv().await.is_err());
	// 	}
	// }
}
