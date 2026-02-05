// Run with: npx tsx tests/merkle-comparison.ts

import { blake2b } from "@noble/hashes/blake2b";

function hash(data: Uint8Array): Uint8Array {
  return blake2b(data, { dkLen: 32 });
}

function hashPair(left: Uint8Array, right: Uint8Array): Uint8Array {
  const combined = new Uint8Array(64);
  combined.set(left, 0);
  combined.set(right, 32);
  return hash(combined);
}

function toHex(arr: Uint8Array): string {
  return Array.from(arr)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

interface MerkleTree {
  root: Uint8Array;
  layers: Uint8Array[][];
}

function buildMerkleTree(leaves: Uint8Array[]): MerkleTree {
  const hashedLeaves = leaves.map((leaf) => hash(leaf));
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

  return { root: currentLayer[0], layers };
}

function generateProof(tree: MerkleTree, index: number): Uint8Array[] {
  const proof: Uint8Array[] = [];
  let currentIndex = index;

  for (let layerIdx = 0; layerIdx < tree.layers.length - 1; layerIdx++) {
    const layer = tree.layers[layerIdx];
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

function verifyProofJS(
  root: Uint8Array,
  proof: Uint8Array[],
  numberOfLeaves: number,
  leafIndex: number,
  leafHash: Uint8Array
): boolean {
  if (leafIndex >= numberOfLeaves) return false;

  let currentHash = leafHash;
  let position = leafIndex;
  let width = numberOfLeaves;
  let proofIdx = 0;

  while (width > 1) {
    if (position + 1 === width && width % 2 === 1) {
      position = Math.floor(position / 2);
      width = Math.floor((width - 1) / 2) + 1;
      continue;
    }

    if (proofIdx >= proof.length) return false;

    const sibling = proof[proofIdx++];
    if (position % 2 === 1 || position + 1 === width) {
      currentHash = hashPair(sibling, currentHash);
    } else {
      currentHash = hashPair(currentHash, sibling);
    }

    position = Math.floor(position / 2);
    width = Math.floor((width - 1) / 2) + 1;
  }

  return toHex(currentHash) === toHex(root);
}

function verifyProofRustStyle(
  root: Uint8Array,
  proof: Uint8Array[],
  numberOfLeaves: number,
  leafIndex: number,
  leafHash: Uint8Array
): { valid: boolean; finalPos: number; finalWidth: number } {
  if (leafIndex >= numberOfLeaves) {
    return { valid: false, finalPos: -1, finalWidth: -1 };
  }

  let currentHash = leafHash;
  let position = leafIndex;
  let width = numberOfLeaves;

  for (const proofElement of proof) {
    if (position % 2 === 1 || position + 1 === width) {
      currentHash = hashPair(proofElement, currentHash);
    } else {
      currentHash = hashPair(currentHash, proofElement);
    }
    position = Math.floor(position / 2);
    width = Math.floor((width - 1) / 2) + 1;
  }

  return {
    valid: toHex(currentHash) === toHex(root),
    finalPos: position,
    finalWidth: width,
  };
}

function runTests() {
  console.log("=== Merkle Tree Comparison Tests ===\n");

  console.log("Test 1: Simple 3-leaf tree");
  const leaves3 = [
    new TextEncoder().encode("a"),
    new TextEncoder().encode("b"),
    new TextEncoder().encode("c"),
  ];
  const tree3 = buildMerkleTree(leaves3);
  console.log(`  Root: ${toHex(tree3.root)}`);
  console.log(`  Layer sizes: ${tree3.layers.map((l) => l.length).join(" -> ")}`);

  for (let i = 0; i < 3; i++) {
    const proof = generateProof(tree3, i);
    const leafHash = hash(leaves3[i]);
    const jsResult = verifyProofJS(tree3.root, proof, 3, i, leafHash);
    const rustResult = verifyProofRustStyle(tree3.root, proof, 3, i, leafHash);
    console.log(
      `  Leaf ${i}: proof_len=${proof.length}, JS=${jsResult}, Rust=${rustResult.valid} (pos=${rustResult.finalPos}, width=${rustResult.finalWidth})`
    );
  }

  console.log("\nTest 2: 10-leaf tree");
  const leaves10 = "abcdefghij".split("").map((c) => new TextEncoder().encode(c));
  const tree10 = buildMerkleTree(leaves10);
  console.log(`  Root: ${toHex(tree10.root)}`);
  console.log(`  Layer sizes: ${tree10.layers.map((l) => l.length).join(" -> ")}`);

  let allPass10 = true;
  for (let i = 0; i < 10; i++) {
    const proof = generateProof(tree10, i);
    const leafHash = hash(leaves10[i]);
    const jsResult = verifyProofJS(tree10.root, proof, 10, i, leafHash);
    const rustResult = verifyProofRustStyle(tree10.root, proof, 10, i, leafHash);
    if (!jsResult || !rustResult.valid) {
      console.log(
        `  FAIL Leaf ${i}: proof_len=${proof.length}, JS=${jsResult}, Rust=${rustResult.valid}`
      );
      allPass10 = false;
    }
  }
  if (allPass10) console.log("  All 10 leaves verified successfully");

  console.log("\nTest 3: 100-leaf tree (battleship grid)");
  const leaves100: Uint8Array[] = [];
  for (let i = 0; i < 100; i++) {
    const leaf = new Uint8Array(33);
    for (let j = 0; j < 32; j++) {
      leaf[j] = (i * 7 + j * 13) % 256;
    }
    leaf[32] = i % 5 === 0 ? 1 : 0;
    leaves100.push(leaf);
  }
  const tree100 = buildMerkleTree(leaves100);
  console.log(`  Root: ${toHex(tree100.root)}`);
  console.log(`  Layer sizes: ${tree100.layers.map((l) => l.length).join(" -> ")}`);

  let failures100: number[] = [];
  for (let i = 0; i < 100; i++) {
    const proof = generateProof(tree100, i);
    const leafHash = hash(leaves100[i]);
    const jsResult = verifyProofJS(tree100.root, proof, 100, i, leafHash);
    const rustResult = verifyProofRustStyle(tree100.root, proof, 100, i, leafHash);
    if (!jsResult || !rustResult.valid) {
      failures100.push(i);
      console.log(
        `  FAIL Leaf ${i}: proof_len=${proof.length}, JS=${jsResult}, Rust=${rustResult.valid} (pos=${rustResult.finalPos}, width=${rustResult.finalWidth})`
      );
    }
  }
  if (failures100.length === 0) {
    console.log("  All 100 leaves verified successfully");
  } else {
    console.log(`  ${failures100.length} failures: ${failures100.join(", ")}`);
  }

  console.log("\nTest 4: Edge cases (promoted nodes at indices 99, 49, 24)");
  const promotedIndices = [99, 49, 24];
  for (const idx of promotedIndices) {
    if (idx < 100) {
      const proof = generateProof(tree100, idx);
      const leafHash = hash(leaves100[idx]);
      const jsResult = verifyProofJS(tree100.root, proof, 100, idx, leafHash);
      const rustResult = verifyProofRustStyle(tree100.root, proof, 100, idx, leafHash);
      console.log(
        `  Leaf ${idx}: proof_len=${proof.length}, JS=${jsResult}, Rust=${rustResult.valid}`
      );
      if (proof.length < 7) {
        console.log(`    -> Shorter proof due to promotions`);
      }
    }
  }

  console.log("\nTest 5: Detailed trace for leaf 99");
  const idx = 99;
  const proof99 = generateProof(tree100, idx);
  const leafHash99 = hash(leaves100[idx]);
  console.log(`  Leaf hash: ${toHex(leafHash99)}`);
  console.log(`  Proof length: ${proof99.length}`);
  console.log(`  Proof elements:`);
  proof99.forEach((p, i) => console.log(`    [${i}]: ${toHex(p)}`));

  console.log(`  Rust-style verification trace:`);
  let currentHash = leafHash99;
  let position = idx;
  let width = 100;
  for (let i = 0; i < proof99.length; i++) {
    const proofElement = proof99[i];
    const condition = position % 2 === 1 || position + 1 === width;
    console.log(
      `    Step ${i}: pos=${position}, width=${width}, condition=${condition ? "LEFT" : "RIGHT"}`
    );
    if (condition) {
      currentHash = hashPair(proofElement, currentHash);
    } else {
      currentHash = hashPair(currentHash, proofElement);
    }
    position = Math.floor(position / 2);
    width = Math.floor((width - 1) / 2) + 1;
  }
  console.log(`  Final: pos=${position}, width=${width}`);
  console.log(`  Computed root: ${toHex(currentHash)}`);
  console.log(`  Expected root: ${toHex(tree100.root)}`);
  console.log(`  Match: ${toHex(currentHash) === toHex(tree100.root)}`);
}

runTests();
