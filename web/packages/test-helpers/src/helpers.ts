import rlp from "rlp"

import { keccakFromHexString, keccak } from "ethereumjs-util"
import { MerkleTree } from "merkletreejs"
import { ethers, Event, Wallet } from "ethers"
import _ from "lodash"
import secp256k1 from "secp256k1"
import seedrandom from "seedrandom"
import { keccak256, entropyToMnemonic } from "ethers/lib/utils"
import type { BeefyClient } from "@snowbridge/contract-types"

let readSetBits = (bitfield: ethers.BigNumber[]): number[] => {
    let bits = bitfield
        .map((i) => {
            let bf = BigInt(i.toString()).toString(2).split("")
            while (bf.length < 256) {
                bf.unshift("0")
            }
            return bf.reverse().join("")
        })
        .join("")
        .replace(/0*$/g, "")
        .split("")
        .map((c) => parseInt(c))

    let indices: number[] = []
    for (let [index, value] of bits.entries()) {
        if (value === 1) {
            indices.push(index)
        }
    }

    return indices
}

let encodeLog = (log: Event) => {
    return rlp.encode([log.address, log.topics, log.data]).toString("hex")
}

class ValidatorSet {
    wallets: Wallet[]
    id: number
    root: string
    length: number
    proofs: string[][]

    constructor(id: number, length: number, privateKeys?: string[]) {
        let wallets: Wallet[] = [],
            wallet: Wallet,
            randomSet = true
        if (privateKeys && privateKeys.length) {
            length = privateKeys.length
            randomSet = false
        }
        for (let i = 0; i < length; i++) {
            if (randomSet) {
                wallet = ethers.Wallet.fromMnemonic(
                    entropyToMnemonic(keccak256(Buffer.from(`${i}`)))
                )
            } else {
                wallet = new ethers.Wallet(privateKeys![i])
            }
            wallets.push(wallet)
        }

        let leaves = wallets.map((w) => keccakFromHexString(w.address))
        let tree = new MerkleTree(leaves, keccak, {
            sortLeaves: false,
            sortPairs: false,
        })

        this.wallets = wallets
        this.id = id
        this.root = tree.getHexRoot()
        this.length = length
        this.proofs = leaves.map((leaf, index) => tree.getHexProof(leaf, index))
    }

    createSignatureProof(index: number, commitmentHash: string): BeefyClient.ValidatorProofStruct {
        let wallet = this.wallets[index]
        let signature = secp256k1.ecdsaSign(
            ethers.utils.arrayify(commitmentHash),
            ethers.utils.arrayify(wallet.privateKey)
        )

        let buf = new Uint8Array(signature.signature.buffer)
        let r = buf.slice(0, 32)
        let s = buf.slice(32)

        return {
            v: signature.recid + 27,
            r,
            s,
            index: index,
            account: wallet.address,
            proof: this.proofs[index],
        }
    }
}

function createRandomSubset(population: number, size: number) {
    seedrandom("test", { global: true })
    const all = Array.from(Array(population).keys())
    const participants = _.runInContext()
        .sampleSize(all, size)
        .sort((a, b) => a - b)
    const absentees = all.filter((o) => !participants.includes(o))
    return { participants, absentees }
}

export { encodeLog, createRandomSubset, ValidatorSet, readSetBits }
