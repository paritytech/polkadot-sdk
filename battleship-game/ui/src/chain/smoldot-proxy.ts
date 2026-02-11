import { start } from "smoldot";
import { WebSocketServer, WebSocket as WsWebSocket } from "ws";
import { relayChainSpec, parachainSpec } from "./chainSpecs.ts";

interface ClientState {
  ws: WsWebSocket;
  namespace: number;
  subscriptions: Set<string>;
  followRequestGids: Set<number>;
  followSubscriptions: Set<string>;
  deadSubscriptions: Set<string>;
}

export async function startProxy(port = 9999): Promise<{ port: number; stop: () => void }> {
  (globalThis as any).WebSocket = (await import("ws")).default;

  console.log("[proxy] Starting smoldot...");
  const smoldot = start({
    maxLogLevel: 4,
    logCallback: (level, target, message) => {
      const names = ["", "ERROR", "WARN", "INFO", "DEBUG"];
      if (target.includes("runtime-") && message.startsWith("RT-")) {
        console.log(`[smoldot:${names[level]}:${target}] ${message.slice(0, 400)}`);
      } else if (target.includes("sync-service") || target.includes("block-announce") || target.includes("header-verify") || target.includes("network")) {
        if (level <= 4) console.log(`[smoldot:${names[level]}:${target}] ${message.slice(0, 200)}`);
      } else if (level <= 3) {
        console.log(`[smoldot:${names[level]}:${target}] ${message.slice(0, 200)}`);
      }
    },
  });
  const relay = await smoldot.addChain({ chainSpec: relayChainSpec });
  console.log("[proxy] Relay chain added");
  const para = await smoldot.addChain({ chainSpec: parachainSpec, potentialRelayChains: [relay] });
  console.log("[proxy] Parachain added");

  const clients = new Map<WsWebSocket, ClientState>();
  let nextNamespace = 0;
  let nextGlobalId = 1;
  const globalIdToClient = new Map<number, { client: ClientState; originalId: number | string }>();
  const subscriptionToClient = new Map<string, ClientState>();

  const wss = new WebSocketServer({ port });
  console.log(`[proxy] WebSocket server on ws://127.0.0.1:${port}`);

  wss.on("connection", (ws) => {
    const ns = nextNamespace++;
    const state: ClientState = {
      ws, namespace: ns,
      subscriptions: new Set(),
      followRequestGids: new Set(),
      followSubscriptions: new Set(),
      deadSubscriptions: new Set(),
    };
    clients.set(ws, state);
    console.log(`[proxy] Client ${ns} connected (total: ${clients.size})`);

    ws.on("message", (data) => {
      try {
        const msg = JSON.parse(data.toString());
        const originalId = msg.id;
        const gid = nextGlobalId++;
        globalIdToClient.set(gid, { client: state, originalId });
        const rewritten = { ...msg, id: gid };

        if (msg.method === "chainHead_v1_follow") {
          // Auto-unfollow old subscriptions and mark them dead
          for (const oldSub of state.followSubscriptions) {
            const ufGid = nextGlobalId++;
            para.sendJsonRpc(JSON.stringify({
              jsonrpc: "2.0", id: ufGid,
              method: "chainHead_v1_unfollow", params: [oldSub],
            }));
            subscriptionToClient.delete(oldSub);
            state.subscriptions.delete(oldSub);
            state.deadSubscriptions.add(oldSub);
            console.log(`[proxy] C${ns}: auto-unfollow old ${oldSub.slice(0,8)}... before new follow`);
          }
          state.followSubscriptions.clear();
          state.followRequestGids.add(gid);
          console.log(`[proxy] C${ns} -> smoldot: chainHead_v1_follow gid=${gid}`);
        } else if (msg.method === "chainHead_v1_unfollow") {
          const subId = msg.params?.[0];
          if (subId) {
            state.followSubscriptions.delete(subId);
            state.subscriptions.delete(subId);
            subscriptionToClient.delete(subId);
            state.deadSubscriptions.add(subId);
            console.log(`[proxy] C${ns} -> smoldot: chainHead_v1_unfollow ${subId.slice(0,8)}...`);
          }
        } else if (msg.method !== "chainHead_v1_unpin") {
          const paramStr = msg.method?.includes("storage") ? ` hash=${msg.params?.[0]?.slice(0,16)}...` : "";
          console.log(`[proxy] C${ns} -> smoldot: ${msg.method} gid=${gid}${paramStr}`);
        }

        para.sendJsonRpc(JSON.stringify(rewritten));
      } catch (e) {
        console.error("[proxy] Failed to forward request:", e);
      }
    });

    ws.on("close", () => {
      // Unfollow all chainHead subscriptions to free smoldot slots
      for (const followSub of state.followSubscriptions) {
        console.log(`[proxy] C${ns} disconnect: unfollow ${followSub}`);
        const unfollowGid = nextGlobalId++;
        para.sendJsonRpc(JSON.stringify({
          jsonrpc: "2.0",
          id: unfollowGid,
          method: "chainHead_v1_unfollow",
          params: [followSub],
        }));
        subscriptionToClient.delete(followSub);
      }

      for (const subId of state.subscriptions) {
        subscriptionToClient.delete(subId);
      }
      for (const [gid, entry] of globalIdToClient) {
        if (entry.client === state) globalIdToClient.delete(gid);
      }
      clients.delete(ws);
      console.log(`[proxy] Client ${ns} disconnected (total: ${clients.size})`);
    });
  });

  let totalResponses = 0;
  (async () => {
    while (true) {
      let response: string;
      try {
        response = await para.nextJsonRpcResponse();
      } catch {
        break;
      }
      totalResponses++;
      if (totalResponses <= 10 || totalResponses % 50 === 0) {
        console.log(`[proxy] Response #${totalResponses}: ${response.slice(0, 200)}`);
      }

      try {
        const msg = JSON.parse(response);

        if ("id" in msg && msg.id != null) {
          const gid = msg.id as number;
          const entry = globalIdToClient.get(gid);

          if (entry) {
            globalIdToClient.delete(gid);
            msg.id = entry.originalId;
            const tc = entry.client;

            // Track follow subscription IDs
            if (tc.followRequestGids.has(gid)) {
              tc.followRequestGids.delete(gid);
              if (msg.result && typeof msg.result === "string") {
                tc.followSubscriptions.add(msg.result);
                tc.subscriptions.add(msg.result);
                subscriptionToClient.set(msg.result, tc);
                console.log(`[proxy] C${tc.namespace}: follow sub = ${msg.result}`);
              }
            } else if (msg.result && typeof msg.result === "string" && msg.result.length > 5) {
              tc.subscriptions.add(msg.result);
              subscriptionToClient.set(msg.result, tc);
            }

            if (tc.ws.readyState === WsWebSocket.OPEN) {
              tc.ws.send(JSON.stringify(msg));
            }
          }
          // Silently ignore responses for disconnected clients or cleanup unfollows
        } else if ("params" in msg && msg.params?.subscription) {
          const subId = msg.params.subscription as string;
          const event = msg.params?.result?.event;

          // Drop in-flight events for dead (auto-unfollowed) subscriptions
          let isDead = false;
          for (const c of clients.values()) {
            if (c.deadSubscriptions.has(subId)) {
              isDead = true;
              if (event === "stop") c.deadSubscriptions.delete(subId);
              break;
            }
          }
          if (isDead) continue;

          const targetClient = subscriptionToClient.get(subId);
          if (event) {
            console.log(`[proxy] Notification: ${event} sub=${subId.slice(0,8)}... -> C${targetClient?.namespace ?? "?"}`);
          }
          if (event === "stop") {
            // Smoldot terminated this subscription. Unfollow to free the slot.
            const unfollowGid = nextGlobalId++;
            para.sendJsonRpc(JSON.stringify({
              jsonrpc: "2.0",
              id: unfollowGid,
              method: "chainHead_v1_unfollow",
              params: [subId],
            }));
            console.log(`[proxy] Auto-unfollow after stop: ${subId.slice(0,8)}...`);
            subscriptionToClient.delete(subId);
            if (targetClient) {
              targetClient.followSubscriptions.delete(subId);
              targetClient.subscriptions.delete(subId);
            }
          }
          if (targetClient && targetClient.ws.readyState === WsWebSocket.OPEN) {
            targetClient.ws.send(response);
          }
          // Silently drop notifications for disconnected clients
        } else {
          for (const c of clients.values()) {
            if (c.ws.readyState === WsWebSocket.OPEN) {
              c.ws.send(response);
            }
          }
        }
      } catch {
        for (const c of clients.values()) {
          if (c.ws.readyState === WsWebSocket.OPEN) {
            c.ws.send(response);
          }
        }
      }
    }
  })();

  return {
    port,
    stop: () => {
      wss.close();
      smoldot.terminate();
    },
  };
}

if (typeof process !== "undefined" && process.argv[1]?.includes("smoldot-proxy")) {
  startProxy().then(({ port }) => {
    console.log(`[proxy] Running on port ${port}. Press Ctrl+C to stop.`);
  });
}
