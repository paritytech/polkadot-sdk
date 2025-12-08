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

use sc_transaction_pool_api::{TransactionReceipt, TransactionStatus, TransactionStatusRpc};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReceiptDbError {
	#[error("Database error: {0}")]
	Database(#[from] sqlx::Error),
	#[error("Serialization error: {0}")]
	Serialization(#[from] serde_json::Error),
}

/// Database for storing transaction receipts and their status.
///
/// This provides persistent storage for transaction status information
/// that can be queried via RPC to track transaction lifecycle.
#[derive(Debug, Clone)]
pub struct TransactionReceiptDb {
	pool: SqlitePool,
}

impl TransactionReceiptDb {
	/// Create a new transaction receipt database at the given base path.
	///
	/// # Arguments
	///
	/// * `base_path` - The directory where the SQLite database file will be stored
	///
	/// # Returns
	///
	/// Returns a `Result` with the database instance or a `sqlx::Error` if connection fails
	pub async fn new(base_path: &Path) -> Result<Self, ReceiptDbError> {
		let db_path = base_path.join("transaction_receipts.sqlite");
		let database_url = format!("sqlite:{}", db_path.display());

		let pool = SqlitePoolOptions::new().connect(&database_url).await?;

		// Create tables if they don't exist
		sqlx::query(
			r#"
            CREATE TABLE IF NOT EXISTS transaction_receipts (
                transaction_hash TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                block_hash TEXT,
                block_number INTEGER,
                transaction_index INTEGER,
                events TEXT,
                submitted_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
		)
		.execute(&pool)
		.await?;

		Ok(Self { pool })
	}

	/// Add a pending transaction to the database.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	/// * `submitted_at` - Timestamp when the transaction was submitted (milliseconds since epoch)
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn add_pending_transaction(
		&self,
		tx_hash: &str,
		submitted_at: u64,
	) -> Result<(), ReceiptDbError> {
		let now = chrono::Utc::now().timestamp_millis();

		sqlx::query(
            "INSERT OR REPLACE INTO transaction_receipts (transaction_hash, status, submitted_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(tx_hash)
        .bind("pending")
        .bind(submitted_at as i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

		Ok(())
	}

	/// Update a transaction status to "in_block" when it's included in a block.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	/// * `block_hash` - The hash of the block containing the transaction
	/// * `block_number` - The block number
	/// * `index` - The index of the transaction within the block
	/// * `events` - Status events associated with the transaction
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn update_transaction_in_block(
		&self,
		tx_hash: &str,
		block_hash: &str,
		block_number: u64,
		index: usize,
		events: &[TransactionStatus<String, String>],
	) -> Result<(), ReceiptDbError> {
		let events_json = serde_json::to_string(events)?;
		let now = chrono::Utc::now().timestamp_millis();

		sqlx::query(
            "UPDATE transaction_receipts SET status = 'in_block', block_hash = ?, block_number = ?, transaction_index = ?, events = ?, updated_at = ? WHERE transaction_hash = ?"
        )
        .bind(block_hash)
        .bind(block_number as i64)
        .bind(index as i32)
        .bind(events_json)
        .bind(now)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;

		Ok(())
	}

	/// Update a transaction status to "finalized" when its block is finalized.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	/// * `block_hash` - The hash of the block containing the transaction
	/// * `block_number` - The block number
	/// * `index` - The index of the transaction within the block
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn update_transaction_finalized(
		&self,
		tx_hash: &str,
		block_hash: &str,
		block_number: u64,
		index: usize,
	) -> Result<(), ReceiptDbError> {
		let now = chrono::Utc::now().timestamp_millis();

		sqlx::query(
            "UPDATE transaction_receipts SET status = 'finalized', block_hash = ?, block_number = ?, transaction_index = ?, updated_at = ? WHERE transaction_hash = ?"
        )
        .bind(block_hash)
        .bind(block_number as i64)
        .bind(index as i32)
        .bind(now)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;

		Ok(())
	}

	/// Mark a transaction as dropped from the transaction pool.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn mark_transaction_dropped(&self, tx_hash: &str) -> Result<(), ReceiptDbError> {
		let now = chrono::Utc::now().timestamp_millis();

		sqlx::query(
            "UPDATE transaction_receipts SET status = 'dropped', updated_at = ? WHERE transaction_hash = ?"
        )
        .bind(now)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;

		Ok(())
	}

	/// Mark a transaction as invalid.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn mark_transaction_invalid(&self, tx_hash: &str) -> Result<(), ReceiptDbError> {
		let now = chrono::Utc::now().timestamp_millis();

		sqlx::query(
            "UPDATE transaction_receipts SET status = 'invalid', updated_at = ? WHERE transaction_hash = ?"
        )
        .bind(now)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;

		Ok(())
	}

	/// Retrieve a transaction receipt by its hash.
	///
	/// # Arguments
	///
	/// * `tx_hash` - The transaction hash as a string
	///
	/// # Returns
	///
	/// Returns `Ok(Some(TransactionReceipt))` if found, `Ok(None)` if not found,
	/// or `sqlx::Error` on database error
	pub async fn get_transaction_receipt(
		&self,
		tx_hash: &str,
	) -> Result<Option<TransactionReceipt<String, String>>, ReceiptDbError> {
		let row = sqlx::query(
            "SELECT status, block_hash, block_number, transaction_index, events, submitted_at FROM transaction_receipts WHERE transaction_hash = ?"
        )
        .bind(tx_hash)
        .fetch_optional(&self.pool)
        .await?;

		if let Some(record) = row {
			let status: String = record.get("status");
			let block_hash: Option<String> = record.get("block_hash");
			let block_number: Option<i64> = record.get("block_number");
			let transaction_index: Option<i32> = record.get("transaction_index");
			let events: Option<String> = record.get("events");
			let submitted_at: i64 = record.get("submitted_at");

			let status_rpc = match status.as_str() {
				"pending" => TransactionStatusRpc::InPool,
				"in_block" => TransactionStatusRpc::IncludedInBlock,
				"finalized" => TransactionStatusRpc::Finalized,
				"dropped" => TransactionStatusRpc::Dropped,
				"invalid" => TransactionStatusRpc::Invalid,
				_ => TransactionStatusRpc::InPool,
			};

			let events: Vec<TransactionStatus<String, String>> =
				events.and_then(|e| serde_json::from_str(&e).ok()).unwrap_or_default();

			let receipt = TransactionReceipt {
				status: status_rpc,
				block_hash,
				block_number: block_number.map(|n| n as u64),
				transaction_index: transaction_index.map(|i| i as usize),
				events,
				transaction_hash: tx_hash.to_string(),
				submitted_at: submitted_at as u64,
			};

			Ok(Some(receipt))
		} else {
			Ok(None)
		}
	}

	/// Enhanced method to store transaction events more effectively
	pub async fn store_transaction_events(
		&self,
		tx_hash: &str,
		events: &[sc_transaction_pool_api::TransactionStatus<String, String>],
	) -> Result<(), sqlx::Error> {
		let events_json = serde_json::to_string(events).unwrap_or_default();

		sqlx::query("UPDATE transaction_receipts SET events = ? WHERE transaction_hash = ?")
			.bind(events_json)
			.bind(tx_hash)
			.execute(&self.pool)
			.await?;

		Ok(())
	}

	/// Get just the events for a transaction
	pub async fn get_transaction_events(
		&self,
		tx_hash: &str,
	) -> Result<Option<Vec<sc_transaction_pool_api::TransactionStatus<String, String>>>, sqlx::Error>
	{
		let row = sqlx::query("SELECT events FROM transaction_receipts WHERE transaction_hash = ?")
			.bind(tx_hash)
			.fetch_optional(&self.pool)
			.await?;

		if let Some(record) = row {
			let events: Option<String> = record.get("events");
			Ok(events.and_then(|e| serde_json::from_str(&e).ok()))
		} else {
			Ok(None)
		}
	}

	/// Clean up old transactions from the database.
	///
	/// Removes finalized, dropped, and invalid transactions older than the specified age.
	///
	/// # Arguments
	///
	/// * `max_age_hours` - Maximum age in hours for transactions to keep
	///
	/// # Returns
	///
	/// Returns `Ok(())` on success, or `sqlx::Error` on database error
	pub async fn cleanup_old_transactions(&self, max_age_hours: i64) -> Result<(), sqlx::Error> {
		let cutoff = chrono::Utc::now().timestamp_millis() - (max_age_hours * 60 * 60 * 1000);

		sqlx::query(
            "DELETE FROM transaction_receipts WHERE created_at < ? AND status IN ('finalized', 'dropped', 'invalid')"
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

		Ok(())
	}
}
