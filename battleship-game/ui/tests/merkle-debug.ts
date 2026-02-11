// Quick script to generate deterministic merkle test data for cross-verification with Rust
import { blake2b } from "@noble/hashes/blake2b";

// Generate deterministic salt: salt[i] = blake2b(i as u32 LE)
function deterministicSalt(index: number): Uint8Array {
	const buf = new Uint8Array(4);
	new DataView(buf.buffer).setUint32(0, index, true);
	return blake2b(buf, { dkLen: 32 });
}

interface Cell {
	salt: Uint8Array;
	isOccupied: boolean;
}

function cellToLeaf(cell: Cell): Uint8Array {
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

function toHex(bytes: Uint8Array): string {
	return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

// Build 100 cells: cell 0 is occupied, rest are not
const cells: Cell[] = [];
for (let i = 0; i < 100; i++) {
	cells.push({
		salt: deterministicSalt(i),
		isOccupied: i === 0, // only cell 0 is occupied
	});
}

// Print first few cells
for (let i = 0; i < 3; i++) {
	console.log(
		`Cell[${i}]: salt=${toHex(cells[i].salt)}, occupied=${cells[i].isOccupied}`,
	);
	const leaf = cellToLeaf(cells[i]);
	console.log(`  leaf bytes (33): ${toHex(leaf)}`);
	const leafHash = hash(leaf);
	console.log(`  leaf hash: ${toHex(leafHash)}`);
}

// Build merkle tree
const rawLeaves = cells.map((c) => cellToLeaf(c));
const hashedLeaves = rawLeaves.map((leaf) => hash(leaf));

console.log("\nFirst 3 hashed leaves:");
for (let i = 0; i < 3; i++) {
	console.log(`  hashedLeaf[${i}]: ${toHex(hashedLeaves[i])}`);
}

// Build layers
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

const root = currentLayer[0];
console.log(`\nMerkle root: ${toHex(root)}`);
console.log(`Tree levels: ${layers.length}`);
for (let l = 0; l < layers.length; l++) {
	console.log(`  Layer ${l}: ${layers[l].length} nodes`);
}

// Generate proof for index 0
function generateProof(
	layers: Uint8Array[][],
	index: number,
): Uint8Array[] {
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

const testIndex = 0;
const proof = generateProof(layers, testIndex);
console.log(`\nProof for index ${testIndex} (${proof.length} elements):`);
for (let i = 0; i < proof.length; i++) {
	console.log(`  proof[${i}]: ${toHex(proof[i])}`);
}

// Verify locally
function verifyProof(
	root: Uint8Array,
	proof: Uint8Array[],
	numberOfLeaves: number,
	leafIndex: number,
	leafHash: Uint8Array,
): boolean {
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

const leafHash = hash(cellToLeaf(cells[testIndex]));
const valid = verifyProof(root, proof, 100, testIndex, leafHash);
console.log(`\nLocal verification: ${valid}`);

// Also test index 99 (interesting due to promotions)
const proof99 = generateProof(layers, 99);
console.log(`\nProof for index 99 (${proof99.length} elements):`);
for (let i = 0; i < proof99.length; i++) {
	console.log(`  proof[${i}]: ${toHex(proof99[i])}`);
}
const leafHash99 = hash(cellToLeaf(cells[99]));
const valid99 = verifyProof(root, proof99, 100, 99, leafHash99);
console.log(`Local verification (index 99): ${valid99}`);

// Print data needed for Rust test:
console.log("\n=== DATA FOR RUST TEST ===");
console.log(`const ROOT: &str = "${toHex(root)}";`);
console.log(`// Cell 0: salt=${toHex(cells[0].salt)}, occupied=true`);
console.log(`// Cell 0 leaf (33 bytes): ${toHex(cellToLeaf(cells[0]))}`);
console.log(`// Cell 0 leaf hash: ${toHex(hash(cellToLeaf(cells[0])))}`);
console.log(`// Proof for index 0:`);
for (let i = 0; i < proof.length; i++) {
	console.log(`//   ${toHex(proof[i])}`);
}
