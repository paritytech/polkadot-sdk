import { createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/node";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";

const WS_URL = "ws://localhost:37359";
const provider = getWsProvider(WS_URL);
const client = createClient(withPolkadotSdkCompat(provider));
const api = client.getUnsafeApi();

const games = await api.query.Battleship.Games.getEntries();
console.log(`Total games: ${games.length}`);

const currentBlock = await api.query.System.Number.getValue();
console.log(`Current block: ${currentBlock}`);

for (const entry of games) {
  const gameId = entry.keyArgs[0];
  const game = entry.value;
  const blocksSinceAction = Number(currentBlock) - Number(game.last_action_block);
  const isAbandoned = blocksSinceAction >= 960;
  console.log(`Game #${gameId}: phase=${game.phase.type}, last_action=${game.last_action_block}, blocks_since=${blocksSinceAction} ${isAbandoned ? '🔴 ABANDONED' : ''}`);
}

client.destroy();
process.exit(0);
