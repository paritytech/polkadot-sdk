import { createClient } from "polkadot-api";
import { WebSocketProvider } from "@polkadot-api/ws-provider";

const provider = WebSocketProvider("ws://127.0.0.1:36637");
const client = createClient(provider);
const api = client.getUnsafeApi();

await new Promise((r) => setTimeout(r, 3000));

function toHex(data: Uint8Array): string {
	return Array.from(data, (b) => b.toString(16).padStart(2, "0")).join("");
}

const testSalt =
	"0x0102030405060708091011121314151617181920212223242526272829303132";

try {
	const commitTx = api.tx.Battleship.commit_grid({
		game_id: 0n,
		grid_root: testSalt,
	});
	const commitData = commitTx.getEncodedData() as Uint8Array;
	console.log("commit_grid call:", "0x" + toHex(commitData));
	console.log("commit_grid length:", commitData.length);
} catch (e) {
	console.log("commit_grid error:", e);
}

try {
	const revealTx = api.tx.Battleship.reveal_cell({
		game_id: 0n,
		reveal: {
			cell: {
				salt: testSalt,
				is_occupied: false,
			},
			proof: [
				"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
			],
			coord: { x: 1, y: 2 },
		},
		expected_round: 0,
	});
	const revealData = revealTx.getEncodedData() as Uint8Array;
	console.log("reveal_cell call:", "0x" + toHex(revealData));
	console.log("reveal_cell length:", revealData.length);

	const expectedSaltInCall = testSalt.slice(2);
	const callHex = toHex(revealData);
	const saltPos = callHex.indexOf(expectedSaltInCall);
	console.log("Salt found at position:", saltPos);
} catch (e) {
	console.log("reveal_cell error:", e);
}

client.destroy();
process.exit(0);
