import { ChartJSNodeCanvas } from "chartjs-node-canvas";
import { writeFile } from "fs/promises";
import { Chain } from "./main";

/// Plot the onchain price changes compared to the canonical price over time.
/// Uses the Chain's priceHistory and blockTime to reconstruct the timeline.
/// Outputs a PNG file with the comparison chart.
export async function plotPriceComparison(
	canonicalPrice: (time: number) => number,
	chain: Chain,
	blockTime: number,
	outputPath: string = "price-comparison.png"
): Promise<void> {
	// Collect all price history entries, sorted by block number
	const historyEntries = Array.from(chain.priceHistory.entries())
		.sort((a, b) => a[0] - b[0]);

	// Include the current price for the current block
	const allBlocks = [...historyEntries];
	if (!chain.priceHistory.has(chain.currentBlock)) {
		// Add current price if not already in history
		allBlocks.push([chain.currentBlock, chain.currentPrice]);
	}

	if (allBlocks.length === 0) {
		console.log("No price history to plot.");
		return;
	}

	// Build arrays for plotting
	const canonicalPrices: number[] = [];
	const onchainPrices: number[] = [];
	const times: number[] = [];

	for (const [block, onchainPrice] of allBlocks) {
		const time = block * blockTime;
		const canonical = canonicalPrice(time);

		times.push(time);
		canonicalPrices.push(canonical);
		onchainPrices.push(onchainPrice);
	}

	// Find min/max for scaling
	const allPrices = [...canonicalPrices, ...onchainPrices];
	const minPrice = Math.min(...allPrices);
	const maxPrice = Math.max(...allPrices);

	console.log("\n" + "=".repeat(80));
	console.log("Price Comparison: Canonical vs Onchain");
	console.log("=".repeat(80));
	console.log(`Time range: ${times[0]}s to ${times[times.length - 1]}s`);
	console.log(`Price range: ${minPrice.toFixed(4)} to ${maxPrice.toFixed(4)}`);
	console.log(`Blocks: ${allBlocks.length}`);
	console.log("\n");

	// Create chart configuration
	const width = 1200;
	const height = 600;
	const chartJSNodeCanvas = new ChartJSNodeCanvas({ width, height });

	const configuration = {
		type: "line" as const,
		data: {
			labels: times.map(t => t.toString()),
			datasets: [
				{
					label: "Canonical Price",
					data: canonicalPrices,
					borderColor: "rgb(54, 162, 235)",
					backgroundColor: "rgba(54, 162, 235, 0.1)",
					tension: 0.1,
					fill: false,
				},
				{
					label: "Onchain Price",
					data: onchainPrices,
					borderColor: "rgb(75, 192, 192)",
					backgroundColor: "rgba(75, 192, 192, 0.1)",
					tension: 0.1,
					fill: false,
				},
			],
		},
		options: {
			responsive: true,
			plugins: {
				title: {
					display: true,
					text: "Price Comparison: Canonical vs Onchain",
				},
				legend: {
					display: true,
					position: "top" as const,
				},
			},
			scales: {
				x: {
					title: {
						display: true,
						text: "Time (seconds)",
					},
				},
				y: {
					title: {
						display: true,
						text: "Price",
					},
					min: minPrice * 0.95,
					max: maxPrice * 1.05,
				},
			},
		},
	};

	// Generate chart image
	const imageBuffer = await chartJSNodeCanvas.renderToBuffer(configuration);
	await writeFile(outputPath, imageBuffer);

	console.log(`Chart saved to: ${outputPath}`);
	console.log("=".repeat(80) + "\n");
}
