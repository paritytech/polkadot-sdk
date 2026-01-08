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
use itertools::Itertools;

use crate::LOG_TARGET;
use sc_utils::id_sequence::SeqID;
use sp_core::{traits::SpawnNamed, Bytes, Encode};
pub use sp_statement_store::StatementStore;
use sp_statement_store::{CheckedTopicFilter, Result, Statement, Topic, MAX_TOPICS};
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

/// Messages sent to matcher tasks.
#[derive(Clone, Debug)]
pub enum MatcherMessage {
	/// A new statement has been submitted.
	NewStatement(Statement),
	/// A new subscription has been created.
	Subscribe(SubscriptionInfo),
	/// Unsubscribe the subscription with the given ID.
	Unsubscribe(SeqID),
}

// Handle to manage all subscriptions.
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
					log::info!(
						target: LOG_TARGET,
						"Started statement subscription matcher task"
					);
					loop {
						let res = subscription_matcher_receiver.recv().await;
						match res {
							Ok(MatcherMessage::NewStatement(statement)) => {
								subscriptions.notify_matching_filters(&statement);
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

	// Generate the next unique subscription ID.
	fn next_id(&self) -> SeqID {
		let id = self.id_sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		SeqID::from(id)
	}

	/// Subscribe to statements matching the topic filter.
	pub(crate) fn subscribe(
		&self,
		topic_filter: CheckedTopicFilter,
	) -> (async_channel::Sender<Bytes>, SubscriptionStatementsStream) {
		let next_id = self.next_id();
		let (tx, rx) = async_channel::bounded(128);
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
	// Subscriptions organized by topic for MatchAll filters.
	subscriptions_match_all_by_topic:
		HashMap<Topic, [HashMap<SeqID, SubscriptionInfo>; MAX_TOPICS]>,
	// Subscriptions organized by topic for MatchAny filters.
	subscriptions_match_any_by_topic: HashMap<Topic, HashMap<SeqID, SubscriptionInfo>>,
	// Subscriptions that listen with Any filter (i.e., no topic filtering).
	subscriptions_any: HashMap<SeqID, SubscriptionInfo>,
	// Mapping from subscription ID to topic filter.
	by_sub_id: HashMap<SeqID, CheckedTopicFilter>,
}

// Information about a single subscription.
#[derive(Clone, Debug)]
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
			subscriptions_match_all_by_topic: HashMap::new(),
			subscriptions_match_any_by_topic: HashMap::new(),
			subscriptions_any: HashMap::new(),
			by_sub_id: HashMap::new(),
		}
	}

	// Subscribe a new subscription.
	fn subscribe(&mut self, subscription_info: SubscriptionInfo) {
		self.by_sub_id
			.insert(subscription_info.seq_id, subscription_info.topic_filter.clone());
		match &subscription_info.topic_filter {
			CheckedTopicFilter::Any => {
				self.subscriptions_any
					.insert(subscription_info.seq_id, subscription_info.clone());
				return;
			},
			CheckedTopicFilter::MatchAll(topics) =>
				for topic in topics {
					self.subscriptions_match_all_by_topic
						.entry(*topic)
						.or_insert_with(Default::default)[topics.len() - 1]
						.insert(subscription_info.seq_id, subscription_info.clone());
				},
			CheckedTopicFilter::MatchAny(topics) =>
				for topic in topics {
					self.subscriptions_match_any_by_topic
						.entry(*topic)
						.or_insert_with(Default::default)
						.insert(subscription_info.seq_id, subscription_info.clone());
				},
		};
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

	fn notify_matching_filters(&mut self, statement: &Statement) {
		self.notify_match_all_subscribers_best(statement);
		self.notify_match_any_subscribers(statement);
		self.notify_any_subscribers(statement);
	}

	// Notify all subscribers with MatchAny filters that match the given statement.
	fn notify_match_any_subscribers(&mut self, statement: &Statement) {
		let mut needs_unsubscribing: HashSet<SeqID> = HashSet::new();
		let mut already_notified: HashSet<SeqID> = HashSet::new();

		let bytes_to_send: Bytes = statement.encode().into();
		for statement_topic in statement.topics() {
			if let Some(subscriptions) = self.subscriptions_match_any_by_topic.get(statement_topic)
			{
				for subscription in subscriptions
					.values()
					.filter(|subscription| already_notified.insert(subscription.seq_id))
				{
					self.notify_subscriber(
						subscription,
						bytes_to_send.clone(),
						&mut needs_unsubscribing,
					);
				}
			}
		}

		// Unsubscribe any subscriptions that failed to receive messages, to give them a chance to
		// recover and not miss statements.
		for sub_id in needs_unsubscribing {
			self.unsubscribe(sub_id);
		}
	}

	// Notify all subscribers with MatchAll filters that match the given statement.
	fn notify_match_all_subscribers_best(&mut self, statement: &Statement) {
		let bytes_to_send: Bytes = statement.encode().into();
		let mut needs_unsubscribing: HashSet<SeqID> = HashSet::new();
		let num_topics = statement.topics().len();

		// Check all combinations of topics in the statement to find matching subscriptions.
		// This works well because the maximum allowed topics is small (MAX_TOPICS = 4).
		for num_topics_to_check in 1..=num_topics {
			for combo in statement.topics().iter().combinations(num_topics_to_check) {
				// Find the topic with the fewest subscriptions to minimize the number of checks.
				let Some(Some(topic_with_fewest)) = combo
					.iter()
					.map(|topic| self.subscriptions_match_all_by_topic.get(*topic))
					.min_by_key(|subscriptions| {
						subscriptions.map_or(0, |subscryptions_by_length| {
							subscryptions_by_length[num_topics_to_check - 1].len()
						})
					})
				else {
					return;
				};

				for subscription in topic_with_fewest[num_topics_to_check - 1]
					.values()
					.filter(|subscription| subscription.topic_filter.matches(statement))
				{
					self.notify_subscriber(
						subscription,
						bytes_to_send.clone(),
						&mut needs_unsubscribing,
					);
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

		// Remove subscription from relevant maps.
		for topic in topics {
			// Check both MatchAny and MatchAll maps.
			if let Some(subscriptions) = self.subscriptions_match_any_by_topic.get_mut(topic) {
				subscriptions.remove(&id);
				if subscriptions.is_empty() {
					self.subscriptions_match_any_by_topic.remove(topic);
				}
			}
			if let Some(subscriptions) = self.subscriptions_match_all_by_topic.get_mut(topic) {
				for subscriptions in subscriptions.iter_mut() {
					if subscriptions.remove(&id).is_some() {
						break;
					}
				}

				if subscriptions.iter().all(|s| s.is_empty()) {
					self.subscriptions_match_all_by_topic.remove(topic);
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
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		assert!(subscriptions.subscriptions_match_all_by_topic.contains_key(&topic1));
		assert!(subscriptions.subscriptions_match_all_by_topic.contains_key(&topic2));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(!subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));

		subscriptions.unsubscribe(sub_info1.seq_id);
		assert!(!subscriptions.subscriptions_match_all_by_topic.contains_key(&topic1));
		assert!(!subscriptions.subscriptions_match_all_by_topic.contains_key(&topic2));
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
			topic_filter: CheckedTopicFilter::MatchAny(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());
		assert!(subscriptions.subscriptions_match_any_by_topic.contains_key(&topic1));
		assert!(subscriptions.subscriptions_match_any_by_topic.contains_key(&topic2));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(!subscriptions.subscriptions_any.contains_key(&sub_info1.seq_id));

		subscriptions.unsubscribe(sub_info1.seq_id);
		assert!(!subscriptions.subscriptions_match_all_by_topic.contains_key(&topic1));
		assert!(!subscriptions.subscriptions_match_all_by_topic.contains_key(&topic2));
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

		let statement = signed_statement(1);
		subscriptions.notify_matching_filters(&statement);

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
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic2));
		subscriptions.notify_matching_filters(&statement);

		// Should not receive yet, only one topic matched.
		assert!(rx1.try_recv().is_err());

		statement.set_topic(1, Topic::from(topic1));
		subscriptions.notify_matching_filters(&statement);

		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);
	}

	#[test]
	fn test_notify_match_any_subscribers() {
		let mut subscriptions = SubscriptionsInfo::new();
		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let (tx2, rx2) = async_channel::bounded::<Bytes>(10);

		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};

		let sub_info2 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(vec![topic2].iter().cloned().collect()),
			seq_id: SeqID::from(2),
			tx: tx2,
		};

		subscriptions.subscribe(sub_info1.clone());
		subscriptions.subscribe(sub_info2.clone());

		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));
		statement.set_topic(1, Topic::from(topic2));
		subscriptions.notify_match_any_subscribers(&statement);

		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);

		let received = rx2.try_recv().expect("Should receive statement");
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
					subscriptions_handle.subscribe(CheckedTopicFilter::MatchAll(
						vec![topic1, topic2].iter().cloned().collect(),
					))
				})
				.collect::<Vec<_>>();

			let mut statement = signed_statement(1);
			statement.set_topic(0, Topic::from(topic2));
			subscriptions_handle.notify(statement.clone());

			statement.set_topic(1, Topic::from(topic1));
			subscriptions_handle.notify(statement.clone());

			for (_tx, mut stream) in streams {
				let received = stream.next().await.expect("Should receive statement");
				let decoded_statement: Statement =
					Statement::decode(&mut &received.0[..]).expect("Should decode statement");
				assert_eq!(decoded_statement, statement);
			}
		}
	}

	#[tokio::test]
	async fn test_handle_unsubscribe() {
		let subscriptions_handle =
			SubscriptionsHandle::new(Box::new(sp_core::testing::TaskExecutor::new()), 2);

		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		let (tx, mut stream) = subscriptions_handle.subscribe(CheckedTopicFilter::MatchAll(
			vec![topic1, topic2].iter().cloned().collect(),
		));

		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));
		statement.set_topic(1, Topic::from(topic2));

		// Send a statement and verify it's received.
		subscriptions_handle.notify(statement.clone());

		let received = stream.next().await.expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);

		// Drop the stream to trigger unsubscribe.
		drop(stream);

		// Give some time for the unsubscribe message to be processed.
		tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

		// Send another statement after unsubscribe.
		let mut statement2 = signed_statement(2);
		statement2.set_topic(0, Topic::from(topic1));
		statement2.set_topic(1, Topic::from(topic2));
		subscriptions_handle.notify(statement2.clone());

		// The tx channel should be closed/disconnected since the subscription was removed.
		// Give some time for the notification to potentially arrive (it shouldn't).
		tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

		// The sender should fail to send since the subscription is gone.
		// We verify by checking that the tx channel is disconnected.
		assert!(tx.is_closed(), "Sender should be closed after unsubscribe");
	}

	#[test]
	fn test_unsubscribe_nonexistent() {
		let mut subscriptions = SubscriptionsInfo::new();
		// Unsubscribing a non-existent subscription should not panic.
		subscriptions.unsubscribe(SeqID::from(999));
		// Verify internal state is still valid.
		assert!(subscriptions.by_sub_id.is_empty());
		assert!(subscriptions.subscriptions_any.is_empty());
		assert!(subscriptions.subscriptions_match_all_by_topic.is_empty());
		assert!(subscriptions.subscriptions_match_any_by_topic.is_empty());
	}

	#[test]
	fn test_multiple_subscriptions_same_topic() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let (tx2, rx2) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		let sub_info2 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(2),
			tx: tx2,
		};

		subscriptions.subscribe(sub_info1.clone());
		subscriptions.subscribe(sub_info2.clone());

		// Both subscriptions should be registered under each topic.
		assert_eq!(
			subscriptions
				.subscriptions_match_all_by_topic
				.get(&topic1)
				.unwrap()
				.iter()
				.map(|s| s.len())
				.sum::<usize>(),
			2
		);
		assert_eq!(
			subscriptions
				.subscriptions_match_all_by_topic
				.get(&topic2)
				.unwrap()
				.iter()
				.map(|s| s.len())
				.sum::<usize>(),
			2
		);

		// Send a matching statement.
		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));
		statement.set_topic(1, Topic::from(topic2));
		subscriptions.notify_matching_filters(&statement);

		// Both should receive.
		assert!(rx1.try_recv().is_ok());
		assert!(rx2.try_recv().is_ok());

		// Unsubscribe one.
		subscriptions.unsubscribe(sub_info1.seq_id);

		// Only one subscription should remain.
		assert_eq!(
			subscriptions
				.subscriptions_match_all_by_topic
				.get(&topic1)
				.unwrap()
				.iter()
				.map(|s| s.len())
				.sum::<usize>(),
			1
		);
		assert_eq!(
			subscriptions
				.subscriptions_match_all_by_topic
				.get(&topic2)
				.unwrap()
				.iter()
				.map(|s| s.len())
				.sum::<usize>(),
			1
		);
		assert!(!subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(subscriptions.by_sub_id.contains_key(&sub_info2.seq_id));

		// Send another statement.
		subscriptions.notify_matching_filters(&statement);

		// Only sub2 should receive.
		assert!(rx2.try_recv().is_ok());
		assert!(rx1.try_recv().is_err());
	}

	#[test]
	fn test_subscriber_auto_unsubscribe_on_channel_full() {
		let mut subscriptions = SubscriptionsInfo::new();

		// Create a channel with capacity 1.
		let (tx1, rx1) = async_channel::bounded::<Bytes>(1);
		let topic1 = [8u8; 32];

		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(vec![topic1].iter().cloned().collect()),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));

		// First notification should succeed.
		subscriptions.notify_matching_filters(&statement);
		assert!(rx1.try_recv().is_ok());

		// Fill the channel.
		subscriptions.notify_matching_filters(&statement);
		// Channel is now full.

		// Next notification should trigger auto-unsubscribe.
		subscriptions.notify_matching_filters(&statement);

		// Subscription should be removed.
		assert!(!subscriptions.by_sub_id.contains_key(&sub_info1.seq_id));
		assert!(!subscriptions.subscriptions_match_any_by_topic.contains_key(&topic1));
	}

	#[test]
	fn test_match_any_receives_once_per_statement() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		// Subscribe to MatchAny with both topics.
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		// Create a statement that matches BOTH topics.
		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));
		statement.set_topic(1, Topic::from(topic2));

		subscriptions.notify_match_any_subscribers(&statement);

		// Should receive exactly once, not twice.
		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);

		// No more messages.
		assert!(rx1.try_recv().is_err());
	}

	#[test]
	fn test_match_all_with_single_topic_matches_statement_with_two_topics() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		// Subscribe with MatchAll on only topic1.
		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(vec![topic1].iter().cloned().collect()),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		// Create a statement that has BOTH topic1 and topic2.
		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic1));
		statement.set_topic(1, Topic::from(topic2));

		subscriptions.notify_matching_filters(&statement);

		// Should receive because the statement contains topic1 (which is the only required topic).
		let received = rx1.try_recv().expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);

		// No more messages.
		assert!(rx1.try_recv().is_err());
	}

	#[test]
	fn test_match_all_no_matching_topics() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx1, rx1) = async_channel::bounded::<Bytes>(10);
		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];
		let topic3 = [10u8; 32];

		let sub_info1 = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx1,
		};
		subscriptions.subscribe(sub_info1.clone());

		// Statement with completely different topics.
		let mut statement = signed_statement(1);
		statement.set_topic(0, Topic::from(topic3));

		subscriptions.notify_matching_filters(&statement);

		// Should not receive anything.
		assert!(rx1.try_recv().is_err());
	}

	#[tokio::test]
	async fn test_handle_with_match_any_filter() {
		let subscriptions_handle =
			SubscriptionsHandle::new(Box::new(sp_core::testing::TaskExecutor::new()), 2);

		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		let (_tx, mut stream) = subscriptions_handle.subscribe(CheckedTopicFilter::MatchAny(
			vec![topic1, topic2].iter().cloned().collect(),
		));

		// Statement matching only topic1.
		let mut statement1 = signed_statement(1);
		statement1.set_topic(0, Topic::from(topic1));
		subscriptions_handle.notify(statement1.clone());

		let received = stream.next().await.expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement1);

		// Statement matching only topic2.
		let mut statement2 = signed_statement(2);
		statement2.set_topic(0, Topic::from(topic2));
		subscriptions_handle.notify(statement2.clone());

		let received = stream.next().await.expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement2);
	}

	#[tokio::test]
	async fn test_handle_with_any_filter() {
		let subscriptions_handle =
			SubscriptionsHandle::new(Box::new(sp_core::testing::TaskExecutor::new()), 2);

		let (_tx, mut stream) = subscriptions_handle.subscribe(CheckedTopicFilter::Any);

		// Send statements with various topics.
		let statement1 = signed_statement(1);
		subscriptions_handle.notify(statement1.clone());

		let received = stream.next().await.expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement1);

		let mut statement2 = signed_statement(2);
		statement2.set_topic(0, Topic::from([99u8; 32]));
		subscriptions_handle.notify(statement2.clone());

		let received = stream.next().await.expect("Should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement2);
	}

	#[tokio::test]
	async fn test_handle_multiple_subscribers_different_filters() {
		let subscriptions_handle =
			SubscriptionsHandle::new(Box::new(sp_core::testing::TaskExecutor::new()), 2);

		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		// Subscriber 1: MatchAll on topic1 and topic2.
		let (_tx1, mut stream1) = subscriptions_handle.subscribe(CheckedTopicFilter::MatchAll(
			vec![topic1, topic2].iter().cloned().collect(),
		));

		// Subscriber 2: MatchAny on topic1.
		let (_tx2, mut stream2) = subscriptions_handle
			.subscribe(CheckedTopicFilter::MatchAny(vec![topic1].iter().cloned().collect()));

		// Subscriber 3: Any.
		let (_tx3, mut stream3) = subscriptions_handle.subscribe(CheckedTopicFilter::Any);

		// Statement matching only topic1.
		let mut statement1 = signed_statement(1);
		statement1.set_topic(0, Topic::from(topic1));
		subscriptions_handle.notify(statement1.clone());

		// stream1 should NOT receive (needs both topics).
		// stream2 should receive (MatchAny topic1).
		// stream3 should receive (Any).

		let received2 = stream2.next().await.expect("stream2 should receive");
		let decoded2: Statement = Statement::decode(&mut &received2.0[..]).unwrap();
		assert_eq!(decoded2, statement1);

		let received3 = stream3.next().await.expect("stream3 should receive");
		let decoded3: Statement = Statement::decode(&mut &received3.0[..]).unwrap();
		assert_eq!(decoded3, statement1);

		// Statement matching both topics.
		let mut statement2 = signed_statement(2);
		statement2.set_topic(0, Topic::from(topic1));
		statement2.set_topic(1, Topic::from(topic2));
		subscriptions_handle.notify(statement2.clone());

		// All should receive.
		let received1 = stream1.next().await.expect("stream1 should receive");
		let decoded1: Statement = Statement::decode(&mut &received1.0[..]).unwrap();
		assert_eq!(decoded1, statement2);

		let received2 = stream2.next().await.expect("stream2 should receive");
		let decoded2: Statement = Statement::decode(&mut &received2.0[..]).unwrap();
		assert_eq!(decoded2, statement2);

		let received3 = stream3.next().await.expect("stream3 should receive");
		let decoded3: Statement = Statement::decode(&mut &received3.0[..]).unwrap();
		assert_eq!(decoded3, statement2);
	}

	#[test]
	fn test_statement_without_topics_matches_only_any_filter() {
		let mut subscriptions = SubscriptionsInfo::new();

		let (tx_match_all, rx_match_all) = async_channel::bounded::<Bytes>(10);
		let (tx_match_any, rx_match_any) = async_channel::bounded::<Bytes>(10);
		let (tx_any, rx_any) = async_channel::bounded::<Bytes>(10);

		let topic1 = [8u8; 32];
		let topic2 = [9u8; 32];

		// Subscribe with MatchAll filter.
		let sub_match_all = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAll(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(1),
			tx: tx_match_all,
		};
		subscriptions.subscribe(sub_match_all);

		// Subscribe with MatchAny filter.
		let sub_match_any = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::MatchAny(
				vec![topic1, topic2].iter().cloned().collect(),
			),
			seq_id: SeqID::from(2),
			tx: tx_match_any,
		};
		subscriptions.subscribe(sub_match_any);

		// Subscribe with Any filter.
		let sub_any = SubscriptionInfo {
			topic_filter: CheckedTopicFilter::Any,
			seq_id: SeqID::from(3),
			tx: tx_any,
		};
		subscriptions.subscribe(sub_any);

		// Create a statement without any topics set.
		let statement = signed_statement(1);
		assert!(statement.topics().is_empty(), "Statement should have no topics");

		// Notify all matching filters.
		subscriptions.notify_matching_filters(&statement);

		// Any should receive (matches all statements regardless of topics).
		let received = rx_any.try_recv().expect("Any filter should receive statement");
		let decoded_statement: Statement =
			Statement::decode(&mut &received.0[..]).expect("Should decode statement");
		assert_eq!(decoded_statement, statement);

		// MatchAll should NOT receive (statement has no topics, filter requires topic1 AND topic2).
		assert!(
			rx_match_all.try_recv().is_err(),
			"MatchAll should not receive statement without topics"
		);

		// MatchAny should NOT receive (statement has no topics, filter requires topic1 OR topic2).
		assert!(
			rx_match_any.try_recv().is_err(),
			"MatchAny should not receive statement without topics"
		);
	}
}
