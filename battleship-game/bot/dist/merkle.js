import { blake2b } from "@noble/hashes/blake2b";
import { randomBytes } from "crypto";
function cellToLeaf(cell) {
    const leaf = new Uint8Array(33);
    leaf.set(cell.salt, 0);
    leaf[32] = cell.isOccupied ? 1 : 0;
    return leaf;
}
function hash(data) {
    return blake2b(data, { dkLen: 32 });
}
function hashPair(left, right) {
    const combined = new Uint8Array(64);
    combined.set(left, 0);
    combined.set(right, 32);
    return hash(combined);
}
export function buildMerkleTree(cells) {
    if (cells.length !== 100) {
        throw new Error(`Expected 100 cells, got ${cells.length}`);
    }
    const rawLeaves = cells.map((c) => cellToLeaf(c));
    const hashedLeaves = rawLeaves.map((leaf) => hash(leaf));
    const layers = [hashedLeaves];
    let currentLayer = hashedLeaves;
    while (currentLayer.length > 1) {
        const nextLayer = [];
        for (let i = 0; i < currentLayer.length; i += 2) {
            const left = currentLayer[i];
            if (i + 1 < currentLayer.length) {
                const right = currentLayer[i + 1];
                nextLayer.push(hashPair(left, right));
            }
            else {
                nextLayer.push(left);
            }
        }
        layers.push(nextLayer);
        currentLayer = nextLayer;
    }
    // Generate proofs for all cells
    const proofs = [];
    for (let i = 0; i < cells.length; i++) {
        proofs.push(generateProofForIndex(layers, i));
    }
    return {
        root: currentLayer[0],
        layers,
        proofs,
    };
}
function generateProofForIndex(layers, index) {
    const proof = [];
    let currentIndex = index;
    for (let layerIdx = 0; layerIdx < layers.length - 1; layerIdx++) {
        const layer = layers[layerIdx];
        const isLastInOddLayer = currentIndex === layer.length - 1 && layer.length % 2 === 1;
        if (isLastInOddLayer) {
            currentIndex = Math.floor(currentIndex / 2);
            continue;
        }
        const siblingIndex = currentIndex % 2 === 0 ? currentIndex + 1 : currentIndex - 1;
        proof.push(layer[siblingIndex]);
        currentIndex = Math.floor(currentIndex / 2);
    }
    return proof;
}
export function generateProof(tree, index) {
    return generateProofForIndex(tree.layers, index);
}
export function generateSalt() {
    return new Uint8Array(randomBytes(32));
}
export function createChainCells(occupiedIndices) {
    const cells = [];
    for (let i = 0; i < 100; i++) {
        cells.push({
            salt: generateSalt(),
            isOccupied: occupiedIndices.has(i),
        });
    }
    return cells;
}
export function coordToIndex(x, y) {
    return y * 10 + x;
}
export function getCellLeafHash(cell) {
    return hash(cellToLeaf(cell));
}
