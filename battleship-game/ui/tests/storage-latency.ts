import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { start } from "smoldot";
import { firstValueFrom } from "rxjs";
import { relayChainSpec, parachainSpec } from "../src/chain/chainSpecs.ts";

async function main() {
	const smoldot = start({
		maxLogLevel: 4,
		logCallback: (level, target, message) => {
			if (level <= 2 || (level <= 4 && (
				message.includes("verify-success") ||
				message.includes("verify-error") ||
				message.includes("best block") ||
				message.includes("finalize") ||
				message.includes("Finalized") ||
				message.includes("input-chain")
			))) {
				const names = ["", "ERROR", "WARN", "INFO", "DEBUG"];
				console.log(`[${names[level] || "TRACE"}:${target}] ${message.slice(0, 200)}`);
			}
		},
	});
	const relay = await smoldot.addChain({ chainSpec: relayChainSpec });
	const provider = getSmProvider(() =>
		smoldot.addChain({ chainSpec: parachainSpec, potentialRelayChains: [relay] })
	);
	const client = createClient(provider);

	console.log("Waiting for first best block...");
	const blocks: any = await firstValueFrom(client.bestBlocks$ as any);
	console.log(`Got best block: ${blocks[0]?.hash?.slice(0, 16)}... number=${blocks[0]?.number}`);

	const request = (client as any)._request;

	console.log("\n=== system_accountNextIndex (legacy RPC) ===");
	for (let i = 0; i < 5; i++) {
		const t0 = Date.now();
		const nonce = await request("system_accountNextIndex", ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]);
		console.log(`  ${Date.now() - t0}ms (nonce=${nonce})`);
	}

	const storageKey = "0x62a29c1c1a780e4521b9abe4c1a5e8626b98dc0bde4110ba5fd799e1bc012f30";

	console.log("\n=== state_getStorage ===");
	for (let i = 0; i < 5; i++) {
		const t0 = Date.now();
		const val = await request("state_getStorage", [storageKey]);
		console.log(`  ${Date.now() - t0}ms (val=${val})`);
	}

	console.log("\n=== PAPI typed API (chainHead_v1_storage) ===");
	const api = (client as any).getUnsafeApi();
	for (let i = 0; i < 5; i++) {
		const t0 = Date.now();
		try {
			const nextId = await api.query.Battleship.NextGameId.getValue({ at: "best" });
			console.log(`  ${Date.now() - t0}ms (NextGameId=${nextId}, type=${typeof nextId})`);
		} catch (e: any) {
			console.log(`  ${Date.now() - t0}ms ERROR: ${e?.message?.slice(0, 100)}`);
		}
	}

	console.log("\n=== PAPI PlayerGame query ===");
	for (let i = 0; i < 3; i++) {
		const t0 = Date.now();
		try {
			const pg = await api.query.Battleship.PlayerGame.getValue("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", { at: "best" });
			console.log(`  ${Date.now() - t0}ms (PlayerGame=${pg}, type=${typeof pg})`);
		} catch (e: any) {
			console.log(`  ${Date.now() - t0}ms ERROR: ${e?.message?.slice(0, 100)}`);
		}
	}

	console.log("\n=== Block-triggered state_getStorage (10 blocks) ===");
	let count = 0;
	const sub = (client as any).bestBlocks$.subscribe({
		next: async (blks: any) => {
			if (count >= 10) return;
			count++;
			const t0 = Date.now();
			try {
				await request("state_getStorage", [storageKey]);
				console.log(`  Block ${blks[0]?.number}: ${Date.now() - t0}ms`);
			} catch (e) {
				console.log(`  Block ${blks[0]?.number}: ERROR ${Date.now() - t0}ms`);
			}
		}
	});

	await new Promise(r => setTimeout(r, 30000));
	sub.unsubscribe();

	console.log("\nDone.");
	client.destroy();
	await smoldot.terminate();
	process.exit(0);
}

main().catch(e => { console.error(e); process.exit(1); });
