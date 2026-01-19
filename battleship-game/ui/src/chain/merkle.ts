import { blake2b } from "@noble/hashes/blake2b";
import type { ChainCell } from "../types/index.ts";

// Convert cell to 33-byte leaf (matches pallet's Cell::to_leaf)
// leaf = [32-byte salt || 1-byte is_occupied]
function cellToLeaf(cell: ChainCell): Uint8Array {
  const leaf = new Uint8Array(33);
  leaf.set(cell.salt, 0);
  leaf[32] = cell.isOccupied ? 1 : 0;
  return leaf;
}

// Hash using Blake2b-256 (matches BlakeTwo256 in Substrate)
function hash(data: Uint8Array): Uint8Array {
  return blake2b(data, { dkLen: 32 });
}

// Combine two hashes for internal node (left || right)
function hashPair(left: Uint8Array, right: Uint8Array): Uint8Array {
  const combined = new Uint8Array(64);
  combined.set(left, 0);
  combined.set(right, 32);
  return hash(combined);
}

export interface MerkleTree {
  root: Uint8Array;
  layers: Uint8Array[][];
}

// Build merkle tree from 100 cells
// Matches binary_merkle_tree crate: leaves are hashed, then tree is built
// IMPORTANT: Substrate's binary_merkle_tree PROMOTES odd elements directly,
// it does NOT duplicate them like some other implementations.
export function buildMerkleTree(cells: ChainCell[]): MerkleTree {
  if (cells.length !== 100) {
    throw new Error(`Expected 100 cells, got ${cells.length}`);
  }

  // Get raw leaves (33 bytes each) - these get hashed by the tree algorithm
  const rawLeaves = cells.map((c) => cellToLeaf(c));

  // Hash all raw leaves first (this is what binary_merkle_tree does)
  const hashedLeaves = rawLeaves.map((leaf) => hash(leaf));

  // Build layers from bottom to top
  const layers: Uint8Array[][] = [hashedLeaves];

  let currentLayer = hashedLeaves;

  while (currentLayer.length > 1) {
    const nextLayer: Uint8Array[] = [];

    for (let i = 0; i < currentLayer.length; i += 2) {
      const left = currentLayer[i];
      if (i + 1 < currentLayer.length) {
        // Pair exists - hash them together
        const right = currentLayer[i + 1];
        nextLayer.push(hashPair(left, right));
      } else {
        // Odd element at end - PROMOTE directly (Substrate behavior)
        nextLayer.push(left);
      }
    }

    layers.push(nextLayer);
    currentLayer = nextLayer;
  }

  return {
    root: currentLayer[0],
    layers,
  };
}

// Generate proof for leaf at given index
// Proof consists of sibling hashes from leaf to root
// When a node has no sibling (odd layer, last element), it gets promoted - NO proof element needed
export function generateProof(tree: MerkleTree, index: number): Uint8Array[] {
  const proof: Uint8Array[] = [];
  let currentIndex = index;

  for (let layerIdx = 0; layerIdx < tree.layers.length - 1; layerIdx++) {
    const layer = tree.layers[layerIdx];
    const isLastInOddLayer =
      currentIndex === layer.length - 1 && layer.length % 2 === 1;

    if (isLastInOddLayer) {
      // Node gets promoted directly - no sibling, no proof element
      // Position stays the same relative to next layer
      currentIndex = Math.floor(currentIndex / 2);
      continue;
    }

    // Normal case: get sibling
    const siblingIndex =
      currentIndex % 2 === 0 ? currentIndex + 1 : currentIndex - 1;
    proof.push(layer[siblingIndex]);

    currentIndex = Math.floor(currentIndex / 2);
  }

  return proof;
}

// Verify a proof (for testing)
// Matches Substrate's binary_merkle_tree::verify_proof
export function verifyProof(
  root: Uint8Array,
  proof: Uint8Array[],
  numberOfLeaves: number,
  leafIndex: number,
  leafHash: Uint8Array
): boolean {
  if (leafIndex >= numberOfLeaves) {
    return false;
  }

  let currentHash = leafHash;
  let position = leafIndex;
  let width = numberOfLeaves;
  let proofIdx = 0;

  while (width > 1) {
    // Check if this node gets promoted (last in odd-width layer)
    if (position + 1 === width && width % 2 === 1) {
      // Node promoted directly, no sibling, no proof consumption
      position = Math.floor(position / 2);
      width = Math.floor((width - 1) / 2) + 1;
      continue;
    }

    if (proofIdx >= proof.length) {
      return false;
    }

    const sibling = proof[proofIdx++];
    if (position % 2 === 1) {
      currentHash = hashPair(sibling, currentHash);
    } else {
      currentHash = hashPair(currentHash, sibling);
    }

    position = Math.floor(position / 2);
    width = Math.floor((width - 1) / 2) + 1;
  }

  return arraysEqual(currentHash, root);
}

function arraysEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

// Generate random 32-byte salt
export function generateSalt(): Uint8Array {
  const salt = new Uint8Array(32);
  crypto.getRandomValues(salt);
  return salt;
}

// Create chain cells from board state
export function createChainCells(
  occupiedIndices: Set<number>
): ChainCell[] {
  const cells: ChainCell[] = [];
  for (let i = 0; i < 100; i++) {
    cells.push({
      salt: generateSalt(),
      isOccupied: occupiedIndices.has(i),
    });
  }
  return cells;
}

// Convert grid position to cell index
export function coordToIndex(x: number, y: number): number {
  return y * 10 + x;
}

// Convert cell index to grid position
export function indexToCoord(index: number): { x: number; y: number } {
  return {
    x: index % 10,
    y: Math.floor(index / 10),
  };
}

// Get hash of a cell's leaf for proof verification
export function getCellLeafHash(cell: ChainCell): Uint8Array {
  return hash(cellToLeaf(cell));
}
