import { blake2b } from "@noble/hashes/blake2b";
import { compact } from "scale-ts";
import type { PolkadotSigner } from "polkadot-api/signer";

type SmoldotChain = {
  sendJsonRpc(rpc: string): void;
  nextJsonRpcResponse(): Promise<string>;
};

export interface GameAnnouncement {
  creator: string;
  potAmount: string;
  timestamp: number;
  onChainGameId?: string;
}

export interface JoinResponse {
  joiner: string;
  timestamp: number;
}

const GAME_LOBBY_TOPIC = blake2b("battleship:lobby:v1", { dkLen: 32 });

function creatorChannel(creator: string, timestamp: number): Uint8Array {
  return blake2b(new TextEncoder().encode(`battleship:creator:${creator}:${timestamp}`), { dkLen: 32 });
}

function joinResponseTopic(creator: string, joiner: string, timestamp: number): Uint8Array {
  return blake2b(new TextEncoder().encode(`battleship:join:${creator}:${joiner}:${timestamp}`), { dkLen: 32 });
}

function joinResponseChannel(creator: string, joiner: string, timestamp: number): Uint8Array {
  return blake2b(new TextEncoder().encode(`battleship:join-channel:${creator}:${joiner}:${timestamp}`), { dkLen: 32 });
}

function toHex(bytes: Uint8Array): string {
  return "0x" + Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex: string): Uint8Array {
  const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
  const bytes = new Uint8Array(cleanHex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
  }
  return bytes;
}

function encodeStatementForSigning(
  priority: number,
  channel: Uint8Array,
  topics: Uint8Array[],
  data: Uint8Array
): Uint8Array {
  const parts: Uint8Array[] = [];

  const priorityData = new Uint8Array(5);
  priorityData[0] = 2;
  new DataView(priorityData.buffer).setUint32(1, priority, true);
  parts.push(priorityData);

  const channelData = new Uint8Array(33);
  channelData[0] = 3;
  channelData.set(channel, 1);
  parts.push(channelData);

  for (let i = 0; i < Math.min(topics.length, 4); i++) {
    const topicData = new Uint8Array(33);
    topicData[0] = 4 + i;
    topicData.set(topics[i], 1);
    parts.push(topicData);
  }

  const lenEnc = compact.enc(data.length);
  const dataField = new Uint8Array(1 + lenEnc.length + data.length);
  dataField[0] = 8;
  dataField.set(lenEnc, 1);
  dataField.set(data, 1 + lenEnc.length);
  parts.push(dataField);

  const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
  const result = new Uint8Array(totalLen);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

function encodeStatementWithProof(
  signature: Uint8Array,
  signer: Uint8Array,
  priority: number,
  channel: Uint8Array,
  topics: Uint8Array[],
  data: Uint8Array
): Uint8Array {
  const parts: Uint8Array[] = [];

  // Proof: discriminant(1) + Sr25519 variant(1) + signature(64) + signer(32)
  const proofData = new Uint8Array(1 + 1 + 64 + 32);
  proofData[0] = 0;
  proofData[1] = 0;
  proofData.set(signature, 2);
  proofData.set(signer, 66);
  parts.push(proofData);

  const priorityData = new Uint8Array(5);
  priorityData[0] = 2;
  new DataView(priorityData.buffer).setUint32(1, priority, true);
  parts.push(priorityData);

  const channelData = new Uint8Array(33);
  channelData[0] = 3;
  channelData.set(channel, 1);
  parts.push(channelData);

  for (let i = 0; i < Math.min(topics.length, 4); i++) {
    const topicData = new Uint8Array(33);
    topicData[0] = 4 + i;
    topicData.set(topics[i], 1);
    parts.push(topicData);
  }

  const lenEnc = compact.enc(data.length);
  const dataField = new Uint8Array(1 + lenEnc.length + data.length);
  dataField[0] = 8;
  dataField.set(lenEnc, 1);
  dataField.set(data, 1 + lenEnc.length);
  parts.push(dataField);

  const numFields = parts.length;
  const totalLen = parts.reduce((sum, p) => sum + p.length, 0);
  const lenPrefix = compact.enc(numFields);
  const result = new Uint8Array(lenPrefix.length + totalLen);
  result.set(lenPrefix, 0);
  let offset = lenPrefix.length;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

function extractDataFromStatement(statementBytes: Uint8Array): Uint8Array | null {
  const dataStart = statementBytes.lastIndexOf(0x08);
  if (dataStart === -1) return null;

  let offset = dataStart + 1;
  const firstByte = statementBytes[offset];
  let dataLen: number;
  // SCALE compact encoding: 2 LSBs encode the mode
  if ((firstByte & 0b11) === 0b00) {
    dataLen = firstByte >> 2;
    offset += 1;
  } else if ((firstByte & 0b11) === 0b01) {
    dataLen = (statementBytes[offset] | (statementBytes[offset + 1] << 8)) >> 2;
    offset += 2;
  } else {
    return null;
  }

  return statementBytes.slice(offset, offset + dataLen);
}

let rpcId = 1;

export class StatementStoreClient {
  private chain: SmoldotChain;
  private receivedStatements: Map<string, string> = new Map();
  private listening = false;
  private pendingRequests: Map<string, { resolve: (v: unknown) => void; reject: (e: Error) => void }> = new Map();

  constructor(chain: SmoldotChain) {
    this.chain = chain;
    this.startListening();
  }

  private async startListening(): Promise<void> {
    this.listening = true;

    const topicHex = toHex(GAME_LOBBY_TOPIC);
    const subscribeId = String(rpcId++);
    this.pendingRequests.set(subscribeId, {
      resolve: (result) => console.log("Statement subscribe OK:", result),
      reject: (err) => console.error("Statement subscribe FAILED:", err.message),
    });
    this.chain.sendJsonRpc(JSON.stringify({
      jsonrpc: "2.0", id: subscribeId, method: "statement_subscribe", params: [[topicHex]]
    }));

    while (this.listening) {
      let msg: string;
      try {
        msg = await this.chain.nextJsonRpcResponse();
      } catch {
        break;
      }

      try {
        const parsed = JSON.parse(msg);

        if (parsed.id != null) {
          const pending = this.pendingRequests.get(String(parsed.id));
          if (pending) {
            this.pendingRequests.delete(String(parsed.id));
            if (parsed.error) {
              pending.reject(new Error(parsed.error.message));
            } else {
              pending.resolve(parsed.result);
            }
          } else if (parsed.error) {
            console.error("Statement RPC error:", parsed.error.message);
          }
          continue;
        }

        if (parsed.params?.result) {
          const statementHex = parsed.params.result;
          console.log("Statement notification received:", statementHex.substring(0, 40) + "...");
          this.receivedStatements.set(statementHex, statementHex);
        }
      } catch {
        continue;
      }
    }
  }

  private sendRequest(method: string, params: unknown[]): Promise<unknown> {
    const id = String(rpcId++);
    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      this.chain.sendJsonRpc(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    });
  }

  async announceGame(
    announcement: GameAnnouncement,
    signer: PolkadotSigner,
    priority: number = 100
  ): Promise<boolean> {
    try {
      const data = new TextEncoder().encode(JSON.stringify(announcement));
      const channel = creatorChannel(announcement.creator, announcement.timestamp);

      const signingPayload = encodeStatementForSigning(priority, channel, [GAME_LOBBY_TOPIC], data);
      const signature = await signer.signBytes(signingPayload);

      const statement = encodeStatementWithProof(
        signature,
        signer.publicKey,
        priority,
        channel,
        [GAME_LOBBY_TOPIC],
        data
      );

      const hex = toHex(statement);
      const result = await this.sendRequest("statement_submit", [hex]) as { status: string };
      console.log("Statement submit result:", result);

      this.receivedStatements.set(hex, hex);

      return true;
    } catch (e) {
      console.error("Failed to announce game:", e);
      return false;
    }
  }

  async getAvailableGames(): Promise<GameAnnouncement[]> {
    const games: GameAnnouncement[] = [];
    const now = Date.now();
    const maxAge = 5 * 60 * 1000;

    for (const [, statementHex] of this.receivedStatements) {
      try {
        const statementBytes = fromHex(statementHex);
        const dataBytes = extractDataFromStatement(statementBytes);
        if (!dataBytes) continue;

        const announcement = JSON.parse(new TextDecoder().decode(dataBytes)) as GameAnnouncement;
        if (announcement?.creator && announcement?.timestamp && now - announcement.timestamp < maxAge) {
          if (!("joiner" in announcement)) {
            games.push(announcement);
          }
        }
      } catch {
        continue;
      }
    }

    return games.sort((a, b) => b.timestamp - a.timestamp);
  }

  async sendJoinResponse(
    creator: string,
    creatorTimestamp: number,
    joiner: string,
    signer: PolkadotSigner
  ): Promise<boolean> {
    try {
      const response: JoinResponse = { joiner, timestamp: Date.now() };
      const data = new TextEncoder().encode(JSON.stringify(response));
      const topic = joinResponseTopic(creator, joiner, creatorTimestamp);
      const channel = joinResponseChannel(creator, joiner, creatorTimestamp);
      const priority = 100;

      const signingPayload = encodeStatementForSigning(priority, channel, [topic], data);
      const signature = await signer.signBytes(signingPayload);

      const statement = encodeStatementWithProof(
        signature,
        signer.publicKey,
        priority,
        channel,
        [topic],
        data
      );

      const hex = toHex(statement);
      const result = await this.sendRequest("statement_submit", [hex]) as { status: string };
      console.log("Join response submit result:", result);

      this.receivedStatements.set(hex, hex);

      return true;
    } catch (e) {
      console.error("Failed to send join response:", e);
      return false;
    }
  }

  async getJoinResponses(creator: string, creatorTimestamp: number): Promise<JoinResponse[]> {
    const responses: JoinResponse[] = [];
    const now = Date.now();
    const maxAge = 5 * 60 * 1000;

    for (const [, statementHex] of this.receivedStatements) {
      try {
        const statementBytes = fromHex(statementHex);
        const dataBytes = extractDataFromStatement(statementBytes);
        if (!dataBytes) continue;

        const parsed = JSON.parse(new TextDecoder().decode(dataBytes));
        if (parsed.joiner && parsed.timestamp && now - parsed.timestamp < maxAge) {
          const checkTopic = joinResponseTopic(creator, parsed.joiner, creatorTimestamp);
          const checkHex = toHex(checkTopic);
          if (statementHex.includes(checkHex.slice(2))) {
            responses.push(parsed as JoinResponse);
          }
        }
      } catch {
        continue;
      }
    }

    return responses.sort((a, b) => a.timestamp - b.timestamp);
  }

  destroy(): void {
    this.listening = false;
  }
}

let statementStoreInstance: StatementStoreClient | null = null;

export function getStatementStore(chain: SmoldotChain): StatementStoreClient {
  if (!statementStoreInstance) {
    statementStoreInstance = new StatementStoreClient(chain);
  }
  return statementStoreInstance;
}

export function resetStatementStore(): void {
  if (statementStoreInstance) {
    statementStoreInstance.destroy();
  }
  statementStoreInstance = null;
}
