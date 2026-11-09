import { writeFileSync } from "fs";
import type { WeightSummary } from "./test-case";

/**
 * Exports weight data to an interactive HTML chart using Chart.js.
 *
 * Creates a chart with three lines:
 * 1. Sum of authorship data (header + extrinsics + proof) in KB
 * 2. Compressed authorship data in KB
 * 3. Onchain mandatory proof_size in KB
 *
 * Also annotates blocks that have matched events.
 *
 * @param paraSummary - Map of block numbers to weight summary data
 * @param outputPath - Path where the HTML file should be saved (default: './weight-diagram.html')
 */
export function exportWeightDiagram(
	paraSummary: Map<number, WeightSummary>,
	outputPath: string = "./weight-diagram.html"
): void {
	const blocks = Array.from(paraSummary.keys()).sort((a, b) => a - b);

	// Prepare data arrays
	const authorshipSumData: (number | null)[] = [];
	const compressedData: (number | null)[] = [];
	const onchainMandatoryProofData: number[] = [];
	const eventAnnotations: string[] = [];
	const blockEventMap: { [key: number]: string } = {};

	for (const block of blocks) {
		const summary = paraSummary.get(block)!;

		// Calculate authorship sum (header + extrinsics + proof)
		if (summary.authorshipWeights) {
			const sum =
				summary.authorshipWeights.header +
				summary.authorshipWeights.extrinsics +
				summary.authorshipWeights.proof;
			authorshipSumData.push(sum);
			compressedData.push(summary.authorshipWeights.compressed);
		} else {
			authorshipSumData.push(null);
			compressedData.push(null);
		}

		// Onchain mandatory proof_size (convert from bytes to KB)
		const mandatoryProofKb = Number(summary.onchainWeights.mandatory.proof_size) / 1024;
		onchainMandatoryProofData.push(mandatoryProofKb);

		// Create event annotation if there are matched events
		if (summary.matchedEvent.length > 0) {
			const events = summary.matchedEvent
				.map(e => `${e.module}::${e.event}`)
				.join(", ");
			eventAnnotations.push(`Block ${block}: ${events}`);
			blockEventMap[block] = events;
		}
	}

	// Generate HTML with Chart.js
	const html = `<!DOCTYPE html>
<html lang="en">
<head>
	<meta charset="UTF-8">
	<meta name="viewport" content="width=device-width, initial-scale=1.0">
	<title>Weight Diagram</title>
	<script src="https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js"></script>
	<script src="https://cdn.jsdelivr.net/npm/hammerjs@2.0.8/hammer.min.js"></script>
	<script src="https://cdn.jsdelivr.net/npm/chartjs-plugin-zoom@2.2.0/dist/chartjs-plugin-zoom.min.js"></script>
	<style>
		body {
			font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
			padding: 20px;
			background: #f5f5f5;
		}
		.container {
			max-width: 1400px;
			margin: 0 auto;
			background: white;
			padding: 30px;
			border-radius: 8px;
			box-shadow: 0 2px 8px rgba(0,0,0,0.1);
		}
		h1 {
			margin-top: 0;
			color: #333;
		}
		.chart-container {
			position: relative;
			height: 600px;
			margin-bottom: 30px;
		}
		.controls {
			margin-bottom: 20px;
			display: flex;
			gap: 10px;
		}
		button {
			padding: 8px 16px;
			background: #007bff;
			color: white;
			border: none;
			border-radius: 4px;
			cursor: pointer;
			font-size: 14px;
		}
		button:hover {
			background: #0056b3;
		}
		.events {
			margin-top: 30px;
			padding: 20px;
			background: #f8f9fa;
			border-radius: 4px;
		}
		.events h2 {
			margin-top: 0;
			font-size: 18px;
			color: #333;
		}
		.events ul {
			margin: 10px 0;
			padding-left: 20px;
		}
		.events li {
			margin: 5px 0;
			color: #666;
		}
	</style>
</head>
<body>
	<div class="container">
		<h1>Parachain Weight Diagram</h1>
		<div class="controls">
			<button onclick="chart.resetZoom()">Reset Zoom</button>
		</div>
		<div class="chart-container">
			<canvas id="weightChart"></canvas>
		</div>
		${eventAnnotations.length > 0 ? `
		<div class="events">
			<h2>Matched Events</h2>
			<ul>
				${eventAnnotations.map(e => `<li>${e}</li>`).join('\n\t\t\t\t')}
			</ul>
		</div>
		` : ''}
	</div>
	<script>
		const blockEventMap = ${JSON.stringify(blockEventMap)};
		const blocks = ${JSON.stringify(blocks)};

		const ctx = document.getElementById('weightChart').getContext('2d');
		const chart = new Chart(ctx, {
			type: 'line',
			data: {
				labels: blocks,
				datasets: [
					{
						label: 'Authorship Sum (header + extrinsics + proof) KB',
						data: ${JSON.stringify(authorshipSumData)},
						borderColor: 'rgb(255, 99, 132)',
						backgroundColor: 'rgba(255, 99, 132, 0.1)',
						borderWidth: 2,
						pointRadius: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 8 : 4;
						},
						pointHoverRadius: 10,
						pointStyle: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'star' : 'circle';
						},
						pointBackgroundColor: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'rgb(255, 193, 7)' : 'rgb(255, 99, 132)';
						},
						spanGaps: false
					},
					{
						label: 'Compressed KB',
						data: ${JSON.stringify(compressedData)},
						borderColor: 'rgb(54, 162, 235)',
						backgroundColor: 'rgba(54, 162, 235, 0.1)',
						borderWidth: 2,
						pointRadius: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 8 : 4;
						},
						pointHoverRadius: 10,
						pointStyle: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'star' : 'circle';
						},
						pointBackgroundColor: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'rgb(255, 193, 7)' : 'rgb(54, 162, 235)';
						},
						spanGaps: false
					},
					{
						label: 'Onchain Mandatory Proof Size KB',
						data: ${JSON.stringify(onchainMandatoryProofData)},
						borderColor: 'rgb(75, 192, 192)',
						backgroundColor: 'rgba(75, 192, 192, 0.1)',
						borderWidth: 2,
						pointRadius: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 8 : 4;
						},
						pointHoverRadius: 10,
						pointStyle: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'star' : 'circle';
						},
						pointBackgroundColor: function(context) {
							const blockNum = blocks[context.dataIndex];
							return blockEventMap[blockNum] ? 'rgb(255, 193, 7)' : 'rgb(75, 192, 192)';
						}
					}
				]
			},
			options: {
				responsive: true,
				maintainAspectRatio: false,
				interaction: {
					mode: 'index',
					intersect: false,
				},
				plugins: {
					title: {
						display: true,
						text: 'Weight Analysis by Block',
						font: { size: 18 }
					},
					legend: {
						display: true,
						position: 'top',
					},
					tooltip: {
						callbacks: {
							title: function(context) {
								const blockNum = context[0].label;
								let title = 'Block ' + blockNum;
								if (blockEventMap[blockNum]) {
									title += ' ⭐';
								}
								return title;
							},
							label: function(context) {
								let label = context.dataset.label || '';
								if (label) {
									label += ': ';
								}
								if (context.parsed.y !== null) {
									label += context.parsed.y.toFixed(2) + ' KB';
								}
								return label;
							},
							afterLabel: function(context) {
								if (context.datasetIndex === 2) {
									const blockNum = context.label;
									if (blockEventMap[blockNum]) {
										return ['📌 Event: ' + blockEventMap[blockNum]];
									}
									return '';
								}
							}
						}
					},
					zoom: {
						zoom: {
							wheel: {
								enabled: true,
							},
							pinch: {
								enabled: true
							},
							mode: 'x',
						},
						pan: {
							enabled: true,
							mode: 'x',
						}
					}
				},
				scales: {
					x: {
						title: {
							display: true,
							text: 'Block Number',
							font: { size: 14 }
						}
					},
					y: {
						title: {
							display: true,
							text: 'Size (KB)',
							font: { size: 14 }
						},
						beginAtZero: true
					}
				}
			}
		});
	</script>
</body>
</html>`;

	// Write the HTML file
	writeFileSync(outputPath, html, 'utf-8');
	console.log(`Weight diagram exported to ${outputPath}`);
}
