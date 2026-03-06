import { blake2b } from "@noble/hashes/blake2b";
import type { ChainCell } from "./types.js";
import { randomBytes } from "crypto";

function cellToLeaf(cell: ChainCell): Uint8Array {
  const leaf = new Uint8Array(33);
  leaf.set(cell.salt, 0);
  leaf[32] = cell.isOccupied ? 1 : 0;
  return leaf;
}

function hash(data: Uint8Array): Uint8Array {
  return blake2b(data, { dkLen: 32 });
}

function hashPair(left: Uint8Array, right: Uint8Array): Uint8Array {
  const combined = new Uint8Array(64);
  combined.set(left, 0);
  combined.set(right, 32);
  return hash(combined);
}

export interface MerkleTree {
  root: Uint8Array;
  layers: Uint8Array[][];
  proofs: Uint8Array[][];
}

export function buildMerkleTree(cells: ChainCell[]): MerkleTree {
  if (cells.length !== 100) {
    throw new Error(`Expected 100 cells, got ${cells.length}`);
  }

  const rawLeaves = cells.map((c) => cellToLeaf(c));
  const hashedLeaves = rawLeaves.map((leaf) => hash(leaf));
  const layers: Uint8Array[][] = [hashedLeaves];
  let currentLayer = hashedLeaves;

  while (currentLayer.length > 1) {
    const nextLayer: Uint8Array[] = [];

    for (let i = 0; i < currentLayer.length; i += 2) {
      const left = currentLayer[i];
      if (i + 1 < currentLayer.length) {
        const right = currentLayer[i + 1];
        nextLayer.push(hashPair(left, right));
      } else {
        nextLayer.push(left);
      }
    }

    layers.push(nextLayer);
    currentLayer = nextLayer;
  }

  // Generate proofs for all cells
  const proofs: Uint8Array[][] = [];
  for (let i = 0; i < cells.length; i++) {
    proofs.push(generateProofForIndex(layers, i));
  }

  return {
    root: currentLayer[0],
    layers,
    proofs,
  };
}

function generateProofForIndex(layers: Uint8Array[][], index: number): Uint8Array[] {
  const proof: Uint8Array[] = [];
  let currentIndex = index;

  for (let layerIdx = 0; layerIdx < layers.length - 1; layerIdx++) {
    const layer = layers[layerIdx];
    const isLastInOddLayer =
      currentIndex === layer.length - 1 && layer.length % 2 === 1;

    if (isLastInOddLayer) {
      currentIndex = Math.floor(currentIndex / 2);
      continue;
    }

    const siblingIndex =
      currentIndex % 2 === 0 ? currentIndex + 1 : currentIndex - 1;
    proof.push(layer[siblingIndex]);

    currentIndex = Math.floor(currentIndex / 2);
  }

  return proof;
}

export function generateProof(tree: MerkleTree, index: number): Uint8Array[] {
  return generateProofForIndex(tree.layers, index);
}

export function generateSalt(): Uint8Array {
  return new Uint8Array(randomBytes(32));
}

export function createChainCells(occupiedIndices: Set<number>): ChainCell[] {
  const cells: ChainCell[] = [];
  for (let i = 0; i < 100; i++) {
    cells.push({
      salt: generateSalt(),
      isOccupied: occupiedIndices.has(i),
    });
  }
  return cells;
}

export function coordToIndex(x: number, y: number): number {
  return y * 10 + x;
}

export function getCellLeafHash(cell: ChainCell): Uint8Array {
  return hash(cellToLeaf(cell));
}
