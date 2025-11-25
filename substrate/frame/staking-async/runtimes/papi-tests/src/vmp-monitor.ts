/*
 * Vertical Message Passing (VMP) Monitor
 *
 * This tool monitors both Downward Message Passing (DMP) and Upward Message Passing (UMP) queues
 * in the Polkadot relay chain and parachains.
 *
 * ## Message Flow Overview
 *
 * ### Downward Message Passing (DMP): Relay Chain → Parachain
 *
 * 1. **Message Creation**: Messages are created on the relay chain (e.g., XCM messages from governance)
 * 2. **Queueing**: Messages are stored in `Dmp::DownwardMessageQueues` storage on the relay chain
 *    - Each parachain has its own queue indexed by ParaId
 *    - Messages include the actual message bytes and the block number when sent
 * 3. **Delivery**: During parachain validation, these messages are included in the parachain's
 *    inherent data and delivered to the parachain
 * 4. **Processing**: The parachain processes these messages in `parachain-system` pallet
 *    - Messages are passed to the configured `DmpQueue` handler
 *    - In modern implementations, this is typically the `message-queue` pallet
 * 5. **Fee Management**: `Dmp::DeliveryFeeFactor` tracks fee multipliers per parachain
 *
 * ### Upward Message Passing (UMP): Parachain → Relay Chain
 *
 * 1. **Message Creation**: Messages are created on the parachain (e.g., XCM messages)
 * 2. **Parachain Queueing**: Messages are first stored in `ParachainSystem::PendingUpwardMessages`
 *    - The parachain tracks bandwidth limits and adjusts fee factors based on queue size
 * 3. **Commitment**: In `on_finalize`, pending messages are moved to `ParachainSystem::UpwardMessages`
 *    - These are included in the parachain block's proof of validity
 * 4. **Relay Chain Reception**: When the relay chain validates the parachain block:
 *    - UMP messages are extracted from the proof
 *    - Messages are processed by `inclusion` pallet's `receive_upward_messages`
 * 5. **Processing**: Messages are enqueued into the `message-queue` pallet with origin `Ump(ParaId)`
 *    - The message-queue pallet handles actual execution with weight limits
 *    - Messages can be temporarily or permanently overweight
 *
 * ## Key Components
 *
 * ### Relay Chain Pallets
 * - `dmp`: Manages downward message queues and delivery fees
 * - `inclusion`: Handles parachain block validation and UMP message reception
 * - `message-queue`: Generic message queue processor for various origins (UMP, DMP, HRMP)
 *
 * ### Parachain Pallets
 * - `parachain-system`: Manages UMP message sending and DMP message reception
 * - `message-queue`: Processes received DMP messages (and other message types)
 *
 * ## Storage Layout
 *
 * ### Relay Chain
 * - `Dmp::DownwardMessageQueues`: Map<ParaId, Vec<InboundDownwardMessage>>
 * - `Dmp::DeliveryFeeFactor`: Map<ParaId, FixedU128>
 * - Well-known keys for UMP queue sizes (relay_dispatch_queue_size)
 *
 * ### Parachain
 * - `ParachainSystem::PendingUpwardMessages`: Vec<UpwardMessage>
 * - `ParachainSystem::UpwardMessages`: Vec<UpwardMessage> (cleared each block)
 * - `ParachainSystem::UpwardDeliveryFeeFactor`: FixedU128
 *
 * ## Message Queue Pallet
 *
 * The `message-queue` pallet is a generic, paginated message processor that:
 * - Stores messages in "books" organized by origin (e.g., Ump(ParaId), Dmp)
 * - Each book contains pages of messages to handle large message volumes efficiently
 * - Processes messages with strict weight limits to ensure block production
 * - Handles overweight messages that exceed processing limits
 * - Emits events for processed, overweight, and failed messages
 *
 * ## Bandwidth and Fee Management
 *
 * - Both DMP and UMP implement dynamic fee mechanisms
 * - Fees increase when queues grow large (deterring spam)
 * - Fees decrease when queues are small (encouraging usage)
 * - Bandwidth limits prevent any single parachain from monopolizing message passing
 *
 * ## VMP Message Limits and Risk Analysis
 *
 * There are 4 key categories of limits in the VMP system:
 *
 * ### 1. Single Message Size Limit
 *
 * **DMP (Downward):**
 * - Enforced at: `polkadot/runtime/parachains/src/dmp.rs:189` in `can_queue_downward_message()`
 * - Configuration: `max_downward_message_size`
 * - Check: Rejects if `serialized_len > config.max_downward_message_size`
 *
 * **UMP (Upward):**
 * - Parachain enforcement: `cumulus/pallets/parachain-system/src/lib.rs:1665` in `send_upward_message()`
 * - Relay validation: `polkadot/runtime/parachains/src/inclusion/mod.rs:967` in `check_upward_messages()`
 * - Configuration: `max_upward_message_size` (hard bound: 128KB defined as MAX_UPWARD_MESSAGE_SIZE_BOUND)
 *
 * ### 2. Queue Total Size (Bytes)
 *
 * **DMP:**
 * - Max capacity: `MAX_POSSIBLE_ALLOCATION / max_downward_message_size`
 * - Calculated in: `polkadot/runtime/parachains/src/dmp.rs:318-319` in `dmq_max_length()`
 * - Enforced at: `polkadot/runtime/parachains/src/dmp.rs:194` in `can_queue_downward_message()`
 *
 * **UMP:**
 * - Parachain check: `cumulus/pallets/parachain-system/src/lib.rs:369-373` (respects relay's remaining capacity)
 * - Relay limit: `max_upward_queue_size` enforced at `polkadot/runtime/parachains/src/inclusion/mod.rs:977-980`
 *
 * ### 3. Queue Total Count (Messages)
 *
 * **DMP:**
 * - No explicit total message count limit
 * - Only implicitly limited by total queue size
 *
 * **UMP:**
 * - Relay limit: `max_upward_queue_count` at `polkadot/runtime/parachains/src/inclusion/mod.rs:958-961`
 * - Parachain respects relay's `remaining_count` from `relay_dispatch_queue_remaining_capacity`
 *
 * ### 4. Per-Block Append Limit
 *
 * **DMP:**
 * - No explicit per-block limit for senders
 * - Receivers process up to `processed_downward_messages` per block
 *
 * **UMP:**
 * - Configuration: `max_upward_message_num_per_candidate`
 * - Parachain limit: `cumulus/pallets/parachain-system/src/lib.rs:386` in `on_finalize()`
 * - Relay validation: `polkadot/runtime/parachains/src/inclusion/mod.rs:949-952`
 * - Max bound: 16,384 messages (MAX_UPWARD_MESSAGE_NUM in `polkadot/parachain/src/primitives.rs:436`)
 *
 * ### Receiver-side Risk: Weight Exhaustion
 *
 * Both DMP and UMP messages are processed through the message-queue pallet:
 * - Weight check: substrate/frame/message-queue/src/lib.rs:1591 in `process_message_payload()`
 * - Messages exceeding `overweight_limit` are marked as overweight
 * - Configuration: `ServiceWeight` and `IdleMaxServiceWeight`
 * - Overweight handling: Permanently overweight messages require manual execution via `execute_overweight()`
 *
 * **Key Insight**: DMP is less restrictive with only size-based limits, while UMP implements all four types of limits,
 * providing more granular control over message flow.
 */

import { createClient, type PolkadotClient, type TypedApi } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { rc, parachain } from "@polkadot-api/descriptors";
import { logger } from "./utils";

interface MonitorOptions {
	relayPort: number;
	paraPort?: number;
	refreshInterval: number;
	paraId?: number;
}

interface DmpQueueInfo {
	paraId: number;
	messageCount: number;
	totalSize: number;
	avgMessageSize: number;
	feeFactor: string;
	messages: Array<{
		size: number;
		sentAt: number;
	}>;
}

interface UmpQueueInfo {
	paraId: number;
	relayQueueCount: number;
	relayQueueSize: number;
	pendingCount?: number;
	pendingSize?: number;
}

interface MessageStats {
	dmp: {
		totalQueues: number;
		totalMessages: number;
		totalSize: number;
		avgMessagesPerQueue: number;
		avgSizePerMessage: number;
		queues: DmpQueueInfo[];
	};
	ump: {
		totalParas: number;
		totalMessages: number;
		totalSize: number;
		queues: UmpQueueInfo[];
	};
}

export async function monitorVmpQueues(options: MonitorOptions): Promise<void> {
	const relayWsUrl = `ws://127.0.0.1:${options.relayPort}`;
	const paraWsUrl = options.paraPort ? `ws://127.0.0.1:${options.paraPort}` : null;

	logger.info(`🚀 Connecting to relay chain at ${relayWsUrl}`);
	if (paraWsUrl) {
		logger.info(`🚀 Connecting to parachain at ${paraWsUrl}`);
	}
	logger.info(`📊 Monitoring VMP queues${options.paraId ? ` for parachain ${options.paraId}` : ' for all parachains'}`);
	logger.info(`⏱️  Refresh interval: ${options.refreshInterval}s`);
	logger.info("");

	try {
		// Connect to relay chain
		const relayWsProvider = getWsProvider(relayWsUrl);
		const relayClient = createClient(withPolkadotSdkCompat(relayWsProvider));
		const relayApi = relayClient.getTypedApi(rc);

		// Test relay connection
		const relayChainSpec = await relayClient.getChainSpecData();
		logger.info(`✅ Connected to relay chain: ${relayChainSpec.name}`);

		// Connect to parachain if port provided
		let paraClient: PolkadotClient | null = null;
		let paraApi: any | null = null;
		if (paraWsUrl) {
			const paraWsProvider = getWsProvider(paraWsUrl);
			paraClient = createClient(withPolkadotSdkCompat(paraWsProvider));
			// Use parachain descriptor for the parachain API
			paraApi = paraClient.getTypedApi(parachain);

			const paraChainSpec = await paraClient.getChainSpecData();
			logger.info(`✅ Connected to parachain: ${paraChainSpec.name}`);
		}

		const version = await relayApi.constants.System.Version();
		logger.info(`Relay chain: ${version.spec_name} v${version.spec_version}`);
		logger.info("");

		// Start monitoring loop
		while (true) {
			try {
				await displayMessageStatus(relayApi, paraApi, options.paraId);
			} catch (error) {
				logger.error("Error fetching message data:", error);
			}

			await sleep(options.refreshInterval * 1000);

			// Clear screen for next update
			if (process.stdout.isTTY) {
				process.stdout.write('\x1Bc');
			}
		}

	} catch (error) {
		logger.error("Failed to initialize monitoring:", error);
		process.exit(1);
	}
}

async function displayMessageStatus(relayApi: TypedApi<typeof rc>, paraApi: any | null, specificParaId?: number): Promise<void> {
	const timestamp = new Date().toLocaleString();

	console.log("╔═══════════════════════════════════════════════════════════════╗");
	console.log("║              Vertical Message Passing Monitor                 ║");
	console.log(`║ Last updated: ${timestamp.padEnd(45)} ║`);
	console.log("╚═══════════════════════════════════════════════════════════════╝");
	console.log();

	try {
		const stats = await fetchMessageStats(relayApi, paraApi, specificParaId);

		// Display DMP Statistics
		console.log("📥 DMP (Downward Message Passing) Statistics:");
		console.log(`   Active Queues: ${stats.dmp.totalQueues}`);
		console.log(`   Total Messages: ${stats.dmp.totalMessages}`);
		console.log(`   Total Size: ${formatBytes(stats.dmp.totalSize)}`);
		if (stats.dmp.totalQueues > 0) {
			console.log(`   Avg Messages/Queue: ${stats.dmp.avgMessagesPerQueue.toFixed(1)}`);
		}
		if (stats.dmp.totalMessages > 0) {
			console.log(`   Avg Message Size: ${formatBytes(stats.dmp.avgSizePerMessage)}`);
		}
		console.log();

		// Display UMP Statistics
		console.log("📤 UMP (Upward Message Passing) Statistics:");
		console.log(`   Active Paras: ${stats.ump.totalParas}`);
		console.log(`   Total Messages: ${stats.ump.totalMessages}`);
		console.log(`   Total Size: ${formatBytes(stats.ump.totalSize)}`);
		console.log();

		// Display DMP queue details
		if (stats.dmp.queues.length > 0) {
			console.log("📋 DMP Queue Details:");
			console.log("┌─────────────┬───────────┬─────────────┬─────────────┬─────────────┐");
			console.log("│ Para ID     │ Messages  │ Total Size  │ Avg Size    │ Fee Factor  │");
			console.log("├─────────────┼───────────┼─────────────┼─────────────┼─────────────┤");

			for (const queue of stats.dmp.queues.slice(0, 10)) {
				console.log(
					`│ ${queue.paraId.toString().padEnd(11)} │ ${queue.messageCount.toString().padEnd(9)} │ ${formatBytes(queue.totalSize).padEnd(11)} │ ${formatBytes(queue.avgMessageSize).padEnd(11)} │ ${queue.feeFactor.padEnd(11)} │`
				);
			}

			console.log("└─────────────┴───────────┴─────────────┴─────────────┴─────────────┘");

			if (stats.dmp.queues.length > 10) {
				console.log(`... and ${stats.dmp.queues.length - 10} more DMP queues`);
			}
			console.log();
		}

		// Display UMP queue details
		if (stats.ump.queues.length > 0) {
			console.log("📋 UMP Queue Details:");
			console.log("┌─────────────┬─────────────┬─────────────┬──────────────┬──────────────┐");
			console.log("│ Para ID     │ Relay Msgs  │ Relay Size  │ Pending Msgs │ Pending Size │");
			console.log("├─────────────┼─────────────┼─────────────┼──────────────┼──────────────┤");

			for (const queue of stats.ump.queues.slice(0, 10)) {
				const pendingStr = queue.pendingCount !== undefined ? queue.pendingCount.toString() : "N/A";
				const pendingSizeStr = queue.pendingSize !== undefined ? formatBytes(queue.pendingSize) : "N/A";
				console.log(
					`│ ${queue.paraId.toString().padEnd(11)} │ ${queue.relayQueueCount.toString().padEnd(11)} │ ${formatBytes(queue.relayQueueSize).padEnd(11)} │ ${pendingStr.padEnd(12)} │ ${pendingSizeStr.padEnd(12)} │`
				);
			}

			console.log("└─────────────┴─────────────┴─────────────┴──────────────┴──────────────┘");

			if (stats.ump.queues.length > 10) {
				console.log(`... and ${stats.ump.queues.length - 10} more UMP queues`);
			}
			console.log();
		}

		// Show most active queues
		const topDmpQueues = stats.dmp.queues
			.filter(q => q.messages.length > 0)
			.sort((a, b) => b.messageCount - a.messageCount)
			.slice(0, 3);

		if (topDmpQueues.length > 0) {
			console.log("🔥 Most Active DMP Queues:");
			for (const queue of topDmpQueues) {
				console.log(`   Para ${queue.paraId}: ${queue.messageCount} messages, latest at block ${Math.max(...queue.messages.map(m => m.sentAt))}`);
			}
			console.log();
		}

		// Warning thresholds
		const totalMessages = stats.dmp.totalMessages + stats.ump.totalMessages;
		const totalSize = stats.dmp.totalSize + stats.ump.totalSize;

		if (totalMessages > 1000) {
			console.log("⚠️  WARNING: High message count detected!");
		}
		if (totalSize > 10 * 1024 * 1024) { // 10MB
			console.log("⚠️  WARNING: High memory usage detected!");
		}

		// Note about parachain connection
		if (!paraApi && specificParaId) {
			console.log("ℹ️  Note: Connect to parachain with --para-port to see pending UMP messages");
		}

	} catch (error) {
		console.log("❌ Error fetching message statistics:");
		console.log(`   ${error}`);
	}

	console.log();
	console.log("Press Ctrl+C to stop monitoring");
	console.log();
}

async function fetchMessageStats(relayApi: TypedApi<typeof rc>, paraApi: any | null, specificParaId?: number): Promise<MessageStats> {
	// Fetch DMP stats from relay chain
	const [downwardMessageQueues, deliveryFeeFactors] = await Promise.all([
		relayApi.query.Dmp.DownwardMessageQueues.getEntries(),
		relayApi.query.Dmp.DeliveryFeeFactor.getEntries()
	]);

	const dmpQueues: DmpQueueInfo[] = [];
	let dmpTotalMessages = 0;
	let dmpTotalSize = 0;

	// Process DMP queues
	for (const { keyArgs: [paraId], value: messages } of downwardMessageQueues) {
		if (specificParaId !== undefined && paraId !== specificParaId) {
			continue;
		}

		const messageCount = messages.length;
		if (messageCount === 0) continue;

		const messageSizes = messages.map((msg) => {
			return msg.msg.asBytes().length
		});

		const queueTotalSize = messageSizes.reduce((sum: number, size: number) => sum + size, 0);
		const avgMessageSize = messageCount > 0 ? queueTotalSize / messageCount : 0;

		const feeFactorEntry = deliveryFeeFactors.find(entry => entry.keyArgs[0] === paraId);
		const feeFactorRaw = feeFactorEntry?.value || 1_000_000_000_000_000_000n;
		const feeFactorValue = typeof feeFactorRaw === 'bigint' ?
			Number(feeFactorRaw) / 1_000_000_000_000_000_000 :
			typeof feeFactorRaw === 'number' ?
			feeFactorRaw / 1_000_000_000_000_000_000 :
			1.0;
		const feeFactor = feeFactorValue.toFixed(6);

		dmpQueues.push({
			paraId,
			messageCount,
			totalSize: queueTotalSize,
			avgMessageSize,
			feeFactor,
			messages: messages.map((msg: any, idx: number) => ({
				size: messageSizes[idx]!,
				sentAt: msg.sent_at || 0
			}))
		});

		dmpTotalMessages += messageCount;
		dmpTotalSize += queueTotalSize;
	}

	// Sort DMP queues by message count
	dmpQueues.sort((a, b) => b.messageCount - a.messageCount);

	// Fetch UMP stats
	const umpQueues: UmpQueueInfo[] = [];
	let umpTotalMessages = 0;
	let umpTotalSize = 0;

	// Only check UMP for specified paraId when monitoring a specific parachain
	const paraIds = specificParaId ? [specificParaId] : [];

	for (const paraId of paraIds) {
		try {
			// Try to get the relay dispatch queue size from well-known key
			// This is stored by the inclusion pallet when processing UMP messages
			const wellKnownKey = `0x` +
				`3a6865617070616765735f73746f726167653a` + // :heappages_storage:
				`0000` + // twox128("Parachains")
				`0000` + // twox128("RelayDispatchQueueSize")
				`0000` + // twox64(paraId) - simplified, would need proper encoding
				paraId.toString(16).padStart(8, '0');

			// For now, we'll check if the para has any activity in message queue
			// This is a simplified approach - in production you'd query the actual storage
			const umpQueueInfo: UmpQueueInfo = {
				paraId,
				relayQueueCount: 0,
				relayQueueSize: 0
			};

			// If we have parachain connection and it matches our paraId, get pending messages
			if (paraApi && paraId === specificParaId) {
				try {
					const pendingMessages = await paraApi.query.ParachainSystem.PendingUpwardMessages();
					if (pendingMessages) {
						umpQueueInfo.pendingCount = pendingMessages.length;
						umpQueueInfo.pendingSize = pendingMessages.reduce((sum: number, msg: any) => {
							return sum + (Array.isArray(msg) ? msg.length : 0);
						}, 0);
					}
				} catch (error) {
					// Parachain might not have this storage item
				}
			}

			// Only add if there's any activity
			if (umpQueueInfo.relayQueueCount > 0 || umpQueueInfo.pendingCount) {
				umpQueues.push(umpQueueInfo);
				umpTotalMessages += umpQueueInfo.relayQueueCount + (umpQueueInfo.pendingCount || 0);
				umpTotalSize += umpQueueInfo.relayQueueSize + (umpQueueInfo.pendingSize || 0);
			}

		} catch (error) {
			// Continue with next para if this one fails
		}
	}

	return {
		dmp: {
			totalQueues: dmpQueues.length,
			totalMessages: dmpTotalMessages,
			totalSize: dmpTotalSize,
			avgMessagesPerQueue: dmpQueues.length > 0 ? dmpTotalMessages / dmpQueues.length : 0,
			avgSizePerMessage: dmpTotalMessages > 0 ? dmpTotalSize / dmpTotalMessages : 0,
			queues: dmpQueues
		},
		ump: {
			totalParas: umpQueues.length,
			totalMessages: umpTotalMessages,
			totalSize: umpTotalSize,
			queues: umpQueues
		}
	};
}

function formatBytes(bytes: number): string {
	if (bytes === 0) return '0 B';

	const k = 1024;
	const sizes = ['B', 'KB', 'MB', 'GB'];
	const i = Math.floor(Math.log(bytes) / Math.log(k));

	return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function sleep(ms: number): Promise<void> {
	return new Promise(resolve => setTimeout(resolve, ms));
}
