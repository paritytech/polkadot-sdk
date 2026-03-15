import { blake2b } from "@noble/hashes/blake2b";
import { compact } from "scale-ts";
const GAME_LOBBY_TOPIC = blake2b("battleship:lobby:v1", { dkLen: 32 });
function creatorChannel(creator, timestamp) {
    return blake2b(new TextEncoder().encode(`battleship:creator:${creator}:${timestamp}`), { dkLen: 32 });
}
function joinResponseTopic(creator, joiner, timestamp) {
    return blake2b(new TextEncoder().encode(`battleship:join:${creator}:${joiner}:${timestamp}`), { dkLen: 32 });
}
function joinResponseChannel(creator, joiner, timestamp) {
    return blake2b(new TextEncoder().encode(`battleship:join-channel:${creator}:${joiner}:${timestamp}`), { dkLen: 32 });
}
function pingChannel(creator, pinger, gameTimestamp) {
    return blake2b(new TextEncoder().encode(`battleship:ping:${creator}:${pinger}:${gameTimestamp}`), { dkLen: 32 });
}
function pongChannel(creator, pinger, gameTimestamp) {
    return blake2b(new TextEncoder().encode(`battleship:pong:${creator}:${pinger}:${gameTimestamp}`), { dkLen: 32 });
}
function gameCreatedChannel(creator, joiner, gameTimestamp) {
    return blake2b(new TextEncoder().encode(`battleship:game-created:${creator}:${joiner}:${gameTimestamp}`), { dkLen: 32 });
}
function toHex(bytes) {
    return "0x" + Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}
function fromHex(hex) {
    const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
    const bytes = new Uint8Array(cleanHex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
        bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
    }
    return bytes;
}
function encodeStatementForSigning(expirySeconds, priority, channel, topics, data) {
    const parts = [];
    // Expiry field: discriminant(1) + u64 LE(8) = 9 bytes
    // u64 format: (expiration_timestamp_secs << 32) | sequence_number
    const expiryData = new Uint8Array(9);
    expiryData[0] = 2; // Field::Expiry discriminant
    new DataView(expiryData.buffer).setUint32(1, priority, true);
    new DataView(expiryData.buffer).setUint32(5, expirySeconds, true);
    parts.push(expiryData);
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
function encodeStatementWithProof(signature, signer, expirySeconds, priority, channel, topics, data) {
    const parts = [];
    // Proof: discriminant(1) + Sr25519 variant(1) + signature(64) + signer(32)
    const proofData = new Uint8Array(1 + 1 + 64 + 32);
    proofData[0] = 0;
    proofData[1] = 0;
    proofData.set(signature, 2);
    proofData.set(signer, 66);
    parts.push(proofData);
    // Expiry field: discriminant(1) + u64 LE(8) = 9 bytes
    // u64 format: (expiration_timestamp_secs << 32) | sequence_number
    const expiryData = new Uint8Array(9);
    expiryData[0] = 2; // Field::Expiry discriminant
    new DataView(expiryData.buffer).setUint32(1, priority, true);
    new DataView(expiryData.buffer).setUint32(5, expirySeconds, true);
    parts.push(expiryData);
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
function extractDataFromStatement(statementBytes) {
    const dataStart = statementBytes.lastIndexOf(0x08);
    if (dataStart === -1)
        return null;
    let offset = dataStart + 1;
    const firstByte = statementBytes[offset];
    let dataLen;
    // SCALE compact encoding: 2 LSBs encode the mode
    if ((firstByte & 0b11) === 0b00) {
        dataLen = firstByte >> 2;
        offset += 1;
    }
    else if ((firstByte & 0b11) === 0b01) {
        dataLen = (statementBytes[offset] | (statementBytes[offset + 1] << 8)) >> 2;
        offset += 2;
    }
    else {
        return null;
    }
    return statementBytes.slice(offset, offset + dataLen);
}
let rpcId = 1;
export class StatementStoreClient {
    chain;
    receivedStatements = new Map();
    listening = false;
    pendingRequests = new Map();
    receivedAnnouncements = new Map();
    receivedPongs = new Map();
    sentPings = new Set();
    pingCallbacks = [];
    joinRequestCallbacks = [];
    gameCreatedCallbacks = [];
    constructor(chain) {
        this.chain = chain;
        this.startListening();
    }
    async startListening() {
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
            let msg;
            try {
                msg = await this.chain.nextJsonRpcResponse();
            }
            catch {
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
                        }
                        else {
                            pending.resolve(parsed.result);
                        }
                    }
                    else if (parsed.error) {
                        console.error("Statement RPC error:", parsed.error.message);
                    }
                    continue;
                }
                if (parsed.params?.statement) {
                    const statementHex = parsed.params.statement;
                    console.log("Statement notification received:", statementHex.substring(0, 40) + "...");
                    this.receivedStatements.set(statementHex, statementHex);
                    this.processStatement(statementHex);
                }
            }
            catch {
                continue;
            }
        }
    }
    sendRequest(method, params) {
        const id = String(rpcId++);
        return new Promise((resolve, reject) => {
            this.pendingRequests.set(id, { resolve, reject });
            this.chain.sendJsonRpc(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
        });
    }
    async announceGame(announcement, publicKey, rawSign, priority = 100) {
        try {
            const annotated = { ...announcement, type: "announce" };
            const data = new TextEncoder().encode(JSON.stringify(annotated));
            const channel = creatorChannel(announcement.creator, announcement.timestamp);
            const expirySeconds = Math.floor(Date.now() / 1000) + 300;
            const signingPayload = encodeStatementForSigning(expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const signature = await rawSign(signingPayload);
            console.log("[DEBUG] publicKey:", toHex(publicKey));
            console.log("[DEBUG] signingPayload:", toHex(signingPayload));
            console.log("[DEBUG] signature:", toHex(signature));
            const statement = encodeStatementWithProof(signature, publicKey, expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const hex = toHex(statement);
            console.log("[DEBUG] full statement hex:", hex);
            const result = await this.sendRequest("statement_submit", [hex]);
            console.log("Statement submit result:", result);
            this.receivedStatements.set(hex, hex);
            return true;
        }
        catch (e) {
            console.error("Failed to announce game:", e);
            return false;
        }
    }
    async getAvailableGames() {
        const games = [];
        const now = Date.now();
        const maxAge = 5 * 60 * 1000;
        for (const [, statementHex] of this.receivedStatements) {
            try {
                const statementBytes = fromHex(statementHex);
                const dataBytes = extractDataFromStatement(statementBytes);
                if (!dataBytes)
                    continue;
                const announcement = JSON.parse(new TextDecoder().decode(dataBytes));
                if (announcement?.creator && announcement?.timestamp && now - announcement.timestamp < maxAge) {
                    if (!("joiner" in announcement)) {
                        games.push(announcement);
                    }
                }
            }
            catch {
                continue;
            }
        }
        return games.sort((a, b) => b.timestamp - a.timestamp);
    }
    async sendJoinRequest(creator, creatorTimestamp, joiner, publicKey, rawSign) {
        try {
            const request = { type: "join_request", creator, gameTimestamp: creatorTimestamp, joiner, joinTimestamp: Date.now() };
            const data = new TextEncoder().encode(JSON.stringify(request));
            const channel = joinResponseChannel(creator, joiner, creatorTimestamp);
            const priority = 100;
            const expirySeconds = Math.floor(Date.now() / 1000) + 300;
            const signingPayload = encodeStatementForSigning(expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const signature = await rawSign(signingPayload);
            const statement = encodeStatementWithProof(signature, publicKey, expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const hex = toHex(statement);
            const result = await this.sendRequest("statement_submit", [hex]);
            console.log("Join request submit result:", result);
            this.receivedStatements.set(hex, hex);
            return true;
        }
        catch (e) {
            console.error("Failed to send join request:", e);
            return false;
        }
    }
    async sendGameCreated(creator, gameTimestamp, joiner, onChainGameId, publicKey, rawSign) {
        try {
            const notification = { type: "game_created", creator, gameTimestamp, joiner, onChainGameId };
            const data = new TextEncoder().encode(JSON.stringify(notification));
            const channel = gameCreatedChannel(creator, joiner, gameTimestamp);
            const priority = 100;
            const expirySeconds = Math.floor(Date.now() / 1000) + 300;
            const signingPayload = encodeStatementForSigning(expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const signature = await rawSign(signingPayload);
            const statement = encodeStatementWithProof(signature, publicKey, expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const hex = toHex(statement);
            await this.sendRequest("statement_submit", [hex]);
            console.log("Game created notification sent");
            return true;
        }
        catch (e) {
            console.error("Failed to send game created notification:", e);
            return false;
        }
    }
    processStatement(statementHex) {
        try {
            const statementBytes = fromHex(statementHex);
            const dataBytes = extractDataFromStatement(statementBytes);
            if (!dataBytes)
                return;
            const parsed = JSON.parse(new TextDecoder().decode(dataBytes));
            if (parsed.type === "ping") {
                for (const cb of this.pingCallbacks)
                    cb(parsed);
            }
            else if (parsed.type === "pong") {
                const pong = parsed;
                const key = `${pong.creator}:${pong.gameTimestamp}:${pong.pinger}`;
                this.receivedPongs.set(key, pong);
            }
            else if (parsed.type === "join_request") {
                const req = parsed;
                console.log(`[StatementStore] Join request from ${req.joiner.slice(0, 8)}... for game by ${req.creator.slice(0, 8)}...`);
                for (const cb of this.joinRequestCallbacks)
                    cb(req);
            }
            else if (parsed.type === "game_created") {
                const notification = parsed;
                console.log(`[StatementStore] Game created: ${notification.onChainGameId} for ${notification.joiner.slice(0, 8)}...`);
                for (const cb of this.gameCreatedCallbacks)
                    cb(notification);
            }
            else if (parsed.type === "announce" || (parsed.creator && parsed.timestamp && !parsed.type)) {
                const ann = parsed;
                if (ann.creator && ann.timestamp) {
                    const now = Date.now();
                    const maxAge = 5 * 60 * 1000;
                    if (now - ann.timestamp < maxAge) {
                        const key = `${ann.creator}:${ann.timestamp}`;
                        this.receivedAnnouncements.set(key, ann);
                    }
                }
            }
        }
        catch {
            // silently continue on errors
        }
    }
    onPing(callback) {
        this.pingCallbacks.push(callback);
    }
    onJoinRequest(callback) {
        this.joinRequestCallbacks.push(callback);
    }
    onGameCreated(callback) {
        this.gameCreatedCallbacks.push(callback);
    }
    async sendLivenessPing(creator, gameTimestamp, pinger, publicKey, rawSign) {
        const key = `${creator}:${gameTimestamp}`;
        if (this.sentPings.has(key))
            return true;
        try {
            const ping = { type: "ping", creator, gameTimestamp, pinger, pingTimestamp: Date.now() };
            const data = new TextEncoder().encode(JSON.stringify(ping));
            const channel = pingChannel(creator, pinger, gameTimestamp);
            const priority = 100;
            const expirySeconds = Math.floor(Date.now() / 1000) + 300;
            const signingPayload = encodeStatementForSigning(expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const signature = await rawSign(signingPayload);
            const statement = encodeStatementWithProof(signature, publicKey, expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const hex = toHex(statement);
            await this.sendRequest("statement_submit", [hex]);
            this.sentPings.add(key);
            return true;
        }
        catch (e) {
            console.error("Failed to send liveness ping:", e);
            return false;
        }
    }
    async sendLivenessPong(ping, publicKey, rawSign) {
        try {
            const pong = { type: "pong", creator: ping.creator, gameTimestamp: ping.gameTimestamp, pinger: ping.pinger, pongTimestamp: Date.now() };
            const data = new TextEncoder().encode(JSON.stringify(pong));
            const channel = pongChannel(ping.creator, ping.pinger, ping.gameTimestamp);
            const priority = 100;
            const expirySeconds = Math.floor(Date.now() / 1000) + 300;
            const signingPayload = encodeStatementForSigning(expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const signature = await rawSign(signingPayload);
            const statement = encodeStatementWithProof(signature, publicKey, expirySeconds, priority, channel, [GAME_LOBBY_TOPIC], data);
            const hex = toHex(statement);
            await this.sendRequest("statement_submit", [hex]);
            return true;
        }
        catch (e) {
            console.error("Failed to send liveness pong:", e);
            return false;
        }
    }
    hasPong(creator, gameTimestamp, myAddress) {
        const key = `${creator}:${gameTimestamp}:${myAddress}`;
        return this.receivedPongs.has(key);
    }
    getAnnouncements() {
        const now = Date.now();
        const maxAge = 5 * 60 * 1000;
        const result = [];
        for (const [key, ann] of this.receivedAnnouncements) {
            if (now - ann.timestamp < maxAge) {
                result.push(ann);
            }
            else {
                this.receivedAnnouncements.delete(key);
            }
        }
        return result.sort((a, b) => b.timestamp - a.timestamp);
    }
    clearPingState(creator, gameTimestamp) {
        const pingKey = `${creator}:${gameTimestamp}`;
        this.sentPings.delete(pingKey);
        for (const key of this.receivedPongs.keys()) {
            if (key.startsWith(pingKey + ":")) {
                this.receivedPongs.delete(key);
            }
        }
    }
    destroy() {
        this.listening = false;
    }
}
let statementStoreInstance = null;
export function getStatementStore(chain) {
    if (!statementStoreInstance) {
        statementStoreInstance = new StatementStoreClient(chain);
    }
    return statementStoreInstance;
}
export function resetStatementStore() {
    if (statementStoreInstance) {
        statementStoreInstance.destroy();
    }
    statementStoreInstance = null;
}
