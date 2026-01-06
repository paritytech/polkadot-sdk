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
      // If odd number of nodes, duplicate the last one
      const right = i + 1 < currentLayer.length ? currentLayer[i + 1] : left;
      nextLayer.push(hashPair(left, right));
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
export function generateProof(tree: MerkleTree, index: number): Uint8Array[] {
  const proof: Uint8Array[] = [];
  let currentIndex = index;

  for (let layerIdx = 0; layerIdx < tree.layers.length - 1; layerIdx++) {
    const layer = tree.layers[layerIdx];
    const siblingIndex = currentIndex % 2 === 0 ? currentIndex + 1 : currentIndex - 1;

    // Get sibling (use current if sibling doesn't exist - odd layer)
    const sibling =
      siblingIndex < layer.length ? layer[siblingIndex] : layer[currentIndex];
    proof.push(sibling);

    // Move to parent index
    currentIndex = Math.floor(currentIndex / 2);
  }

  return proof;
}

// Verify a proof (for testing)
export function verifyProof(
  root: Uint8Array,
  proof: Uint8Array[],
  leafIndex: number,
  leafHash: Uint8Array
): boolean {
  let currentHash = leafHash;
  let currentIndex = leafIndex;

  for (const sibling of proof) {
    if (currentIndex % 2 === 0) {
      currentHash = hashPair(currentHash, sibling);
    } else {
      currentHash = hashPair(sibling, currentHash);
    }
    currentIndex = Math.floor(currentIndex / 2);
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
