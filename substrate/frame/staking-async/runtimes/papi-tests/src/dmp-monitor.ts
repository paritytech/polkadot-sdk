import { createClient, type PolkadotClient, type TypedApi } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { rc } from "@polkadot-api/descriptors";
import { logger } from "./utils";

interface MonitorOptions {
	port: number;
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

interface DmpStats {
	totalQueues: number;
	totalMessages: number;
	totalSize: number;
	avgMessagesPerQueue: number;
	avgSizePerMessage: number;
	queues: DmpQueueInfo[];
}

export async function monitorDmpQueue(options: MonitorOptions): Promise<void> {
	const wsUrl = `ws://127.0.0.1:${options.port}`;

	logger.info(`🚀 Connecting to chain at ${wsUrl}`);
	logger.info(`📊 Monitoring DMP queues${options.paraId ? ` for parachain ${options.paraId}` : ' for all parachains'}`);
	logger.info(`⏱️  Refresh interval: ${options.refreshInterval}s`);
	logger.info("");

	try {
		const wsProvider = getWsProvider(wsUrl);
		const client = createClient(withPolkadotSdkCompat(wsProvider));
		const api = client.getTypedApi(rc);

		// Test connection by getting chain info
		const chainSpec = await client.getChainSpecData();
		logger.info(`✅ Connected to chain: ${chainSpec.name}`);

		// Get chain version to verify connection
		const version = await api.constants.System.Version();
		logger.info(`Chain: ${version.spec_name} v${version.spec_version}`);
		logger.info("");

		// Start monitoring loop
		while (true) {
			try {
				await displayDmpStatus(api, options.paraId);
			} catch (error) {
				logger.error("Error fetching DMP data:", error);
			}

			await sleep(options.refreshInterval * 1000);

			// Clear screen for next update (fancy CLI experience)
			if (process.stdout.isTTY) {
				process.stdout.write('\x1Bc');
			}
		}

	} catch (error) {
		logger.error("Failed to initialize monitoring:", error);
		process.exit(1);
	}
}

async function displayDmpStatus(api: TypedApi<typeof rc>, specificParaId?: number): Promise<void> {
	const timestamp = new Date().toLocaleString();

	console.log("╔═══════════════════════════════════════════════════════════════╗");
	console.log("║                    DMP Queue Monitor                          ║");
	console.log(`║ Last updated: ${timestamp.padEnd(45)} ║`);
	console.log("╚═══════════════════════════════════════════════════════════════╝");
	console.log();

	try {
		const stats = await fetchDmpStats(api, specificParaId);

		// Display overall stats
		console.log("📈 Overall Statistics:");
		console.log(`   Active Queues: ${stats.totalQueues}`);
		console.log(`   Total Messages: ${stats.totalMessages}`);
		console.log(`   Total Size: ${formatBytes(stats.totalSize)}`);
		if (stats.totalQueues > 0) {
			console.log(`   Avg Messages/Queue: ${stats.avgMessagesPerQueue.toFixed(1)}`);
		}
		if (stats.totalMessages > 0) {
			console.log(`   Avg Message Size: ${formatBytes(stats.avgSizePerMessage)}`);
		}
		console.log();

		if (stats.queues.length === 0) {
			console.log("🔍 No active DMP queues found");
			console.log();
			return;
		}

		// Display queue details
		console.log("📋 Queue Details:");
		console.log("┌─────────────┬───────────┬─────────────┬─────────────┬─────────────┐");
		console.log("│ Para ID     │ Messages  │ Total Size  │ Avg Size    │ Fee Factor  │");
		console.log("├─────────────┼───────────┼─────────────┼─────────────┼─────────────┤");

		for (const queue of stats.queues.slice(0, 20)) { // Limit display to top 20
			console.log(
				`│ ${queue.paraId.toString().padEnd(11)} │ ${queue.messageCount.toString().padEnd(9)} │ ${formatBytes(queue.totalSize).padEnd(11)} │ ${formatBytes(queue.avgMessageSize).padEnd(11)} │ ${queue.feeFactor.padEnd(11)} │`
			);
		}

		console.log("└─────────────┴───────────┴─────────────┴─────────────┴─────────────┘");

		if (stats.queues.length > 20) {
			console.log(`... and ${stats.queues.length - 20} more queues`);
		}
		console.log();

		// Show recent activity for the largest queues
		const topQueues = stats.queues
			.filter(q => q.messages.length > 0)
			.sort((a, b) => b.messageCount - a.messageCount)
			.slice(0, 3);

		if (topQueues.length > 0) {
			console.log("🔥 Most Active Queues:");
			for (const queue of topQueues) {
				console.log(`   Para ${queue.paraId}: ${queue.messageCount} messages, latest at block ${Math.max(...queue.messages.map(m => m.sentAt))}`);
			}
			console.log();
		}

		// Memory usage estimate
		const estimatedMemory = stats.totalSize;
		console.log("💾 Memory Usage Estimate:");
		console.log(`   DMP Queue Storage: ~${formatBytes(estimatedMemory)}`);

		// Warning thresholds
		if (stats.totalMessages > 1000) {
			console.log("⚠️  WARNING: High message count detected!");
		}
		if (estimatedMemory > 10 * 1024 * 1024) { // 10MB
			console.log("⚠️  WARNING: High memory usage detected!");
		}

	} catch (error) {
		console.log("❌ Error fetching DMP statistics:");
		console.log(`   ${error}`);
	}

	console.log();
	console.log("Press Ctrl+C to stop monitoring");
	console.log();
}

async function fetchDmpStats(api: TypedApi<typeof rc>, specificParaId?: number): Promise<DmpStats> {
	// Query all storage items at once for efficiency
	const [downwardMessageQueues, deliveryFeeFactors] = await Promise.all([
		api.query.Dmp.DownwardMessageQueues.getEntries(),
		api.query.Dmp.DeliveryFeeFactor.getEntries()
	]);

	const queues: DmpQueueInfo[] = [];
	let totalMessages = 0;
	let totalSize = 0;

	// Process downward message queues
	for (const { keyArgs: [paraId], value: messages } of downwardMessageQueues) {
		// Skip if we're monitoring a specific para and this isn't it
		if (specificParaId !== undefined && paraId !== specificParaId) {
			continue;
		}

		const messageCount = messages.length;
		if (messageCount === 0) continue;

		// Calculate sizes and stats
		const messageSizes = messages.map((msg: any) => {
			// Extract message size from the encoded data
			// The message structure is: { msg: Vec<u8>, sent_at: BlockNumber }
			const msgBytes = msg.msg || [];
			return Array.isArray(msgBytes) ? msgBytes.length : msgBytes.length || 0;
		});

		const queueTotalSize = messageSizes.reduce((sum: number, size: number) => sum + size, 0);
		const avgMessageSize = messageCount > 0 ? queueTotalSize / messageCount : 0;

		// Find corresponding fee factor
		const feeFactorEntry = deliveryFeeFactors.find(entry => entry.keyArgs[0] === paraId);
		const feeFactorRaw = feeFactorEntry?.value || 1.0;
		const feeFactor = typeof feeFactorRaw === 'number' ?
			feeFactorRaw.toFixed(3) :
			feeFactorRaw.toString();

		queues.push({
			paraId,
			messageCount,
			totalSize: queueTotalSize,
			avgMessageSize,
			feeFactor,
			messages: messages.map((msg: any, idx: number) => ({
				size: messageSizes[idx],
				sentAt: msg.sent_at || 0
			}))
		});

		totalMessages += messageCount;
		totalSize += queueTotalSize;
	}

	// Sort queues by message count (descending)
	queues.sort((a, b) => b.messageCount - a.messageCount);

	return {
		totalQueues: queues.length,
		totalMessages,
		totalSize,
		avgMessagesPerQueue: queues.length > 0 ? totalMessages / queues.length : 0,
		avgSizePerMessage: totalMessages > 0 ? totalSize / totalMessages : 0,
		queues
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
