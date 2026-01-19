import { blake2b } from "@noble/hashes/blake2b";
import { compact } from "scale-ts";
import type { PolkadotClient } from "polkadot-api";
import type { PolkadotSigner } from "polkadot-api/signer";

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

export class StatementStoreClient {
  private client: PolkadotClient;

  constructor(client: PolkadotClient) {
    this.client = client;
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

      const result = await (this.client as unknown as { _request: (method: string, params: unknown[]) => Promise<{ status: string }> })
        ._request("statement_submit", [toHex(statement)]);
      console.log("Statement submit result:", result);
      return result.status === "broadcast" || result.status === "new";
    } catch (e) {
      console.error("Failed to announce game:", e);
      return false;
    }
  }

  async getAvailableGames(): Promise<GameAnnouncement[]> {
    try {
      const topicArray = Array.from(GAME_LOBBY_TOPIC);
      const broadcasts = await (this.client as unknown as { _request: (method: string, params: unknown[]) => Promise<string[]> })
        ._request("statement_broadcasts", [[topicArray]]);

      const games: GameAnnouncement[] = [];
      const now = Date.now();
      const maxAge = 5 * 60 * 1000;

      for (const broadcastHex of broadcasts || []) {
        try {
          const dataBytes = typeof broadcastHex === "string" ? fromHex(broadcastHex) : new Uint8Array(broadcastHex as unknown as number[]);
          const announcement = JSON.parse(new TextDecoder().decode(dataBytes)) as GameAnnouncement;
          if (announcement && now - announcement.timestamp < maxAge) {
            games.push(announcement);
          }
        } catch (e) {
          console.warn("Failed to decode broadcast:", e);
        }
      }

      return games.sort((a, b) => b.timestamp - a.timestamp);
    } catch (e) {
      console.error("Failed to get available games:", e);
      return [];
    }
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

      const result = await (this.client as unknown as { _request: (method: string, params: unknown[]) => Promise<{ status: string }> })
        ._request("statement_submit", [toHex(statement)]);
      console.log("Join response submit result:", result);
      return result.status === "broadcast" || result.status === "new";
    } catch (e) {
      console.error("Failed to send join response:", e);
      return false;
    }
  }

  async getJoinResponses(creator: string, creatorTimestamp: number): Promise<JoinResponse[]> {
    try {
      const responses: JoinResponse[] = [];
      const now = Date.now();
      const maxAge = 5 * 60 * 1000;

      const broadcasts = await (this.client as unknown as { _request: (method: string, params: unknown[]) => Promise<string[]> })
        ._request("statement_dump", []);

      for (const statementHex of broadcasts || []) {
        try {
          const statementBytes = fromHex(statementHex);
          const dataStart = statementBytes.lastIndexOf(0x08);
          if (dataStart === -1) continue;

          let offset = dataStart + 1;
          const firstByte = statementBytes[offset];
          let dataLen: number;
          if ((firstByte & 0b11) === 0b00) {
            dataLen = firstByte >> 2;
            offset += 1;
          } else if ((firstByte & 0b11) === 0b01) {
            dataLen = (statementBytes[offset] | (statementBytes[offset + 1] << 8)) >> 2;
            offset += 2;
          } else {
            continue;
          }

          const dataBytes = statementBytes.slice(offset, offset + dataLen);
          const jsonStr = new TextDecoder().decode(dataBytes);
          const parsed = JSON.parse(jsonStr);

          if (parsed.joiner && parsed.timestamp && now - parsed.timestamp < maxAge) {
            const expectedTopic = joinResponseTopic(creator, parsed.joiner, creatorTimestamp);
            const topicHex = toHex(expectedTopic);
            if (statementHex.includes(topicHex.slice(2))) {
              responses.push(parsed as JoinResponse);
            }
          }
        } catch {
          continue;
        }
      }

      return responses.sort((a, b) => a.timestamp - b.timestamp);
    } catch (e) {
      console.error("Failed to get join responses:", e);
      return [];
    }
  }
}

let statementStoreInstance: StatementStoreClient | null = null;

export function getStatementStore(client: PolkadotClient): StatementStoreClient {
  if (!statementStoreInstance) {
    statementStoreInstance = new StatementStoreClient(client);
  }
  return statementStoreInstance;
}

export function resetStatementStore(): void {
  statementStoreInstance = null;
}
