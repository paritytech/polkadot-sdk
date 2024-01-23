import { ValidatorSet, createRandomSubset, readSetBits } from "./helpers"
import { BigNumber, ethers } from "ethers"
import type { BeefyClient } from "@snowbridge/contract-types"
import { accounts } from "./wallets"
import path from "path"
import fs from "fs"
const encoder = new ethers.utils.AbiCoder()

const generateValidatorProof = async (bitfieldFile: string, validatorProofFile: string, validatorSet: ValidatorSet, commitHash: any) => {
    const testFixture = JSON.parse(fs.readFileSync(bitfieldFile, "utf8"))
    const bitField = encoder.decode(["uint256[]"], testFixture.final.finalBitFieldRaw)[0]
    console.log(bitField)
    let finalBitfield: BigNumber[] = []
    for (let i = 0; i < bitField.length; i++) {
        finalBitfield.push(bitField[i])
    }
    const finalValidatorsProof: BeefyClient.ValidatorProofStruct[] = readSetBits(
        finalBitfield
    ).map((i) => validatorSet.createSignatureProof(i, commitHash))
    console.log("Final Validator proofs:", finalValidatorsProof)
    const finalValidatorsProofRaw = encoder.encode(
        [
            "tuple(uint8 v, bytes32 r, bytes32 s, uint256 index,address account,bytes32[] proof)[]",
        ],
        [finalValidatorsProof]
    )
    fs.writeFileSync(
        validatorProofFile,
        JSON.stringify({ finalValidatorsProof, finalValidatorsProofRaw }, null, 2),
        "utf8"
    )
    console.log("Beefy fixture writing to dest file: " + validatorProofFile)
}

const run = async () => {
    const basedir = process.env.contract_dir || "../../../contracts"
    const fixtureData = JSON.parse(
        fs.readFileSync(path.join(basedir, "test/data/beefy-commitment.json"), "utf8")
    )
    const ValidatorSetFile = path.join(basedir, "test/data/beefy-validator-set.json")
    const BitFieldFile0SigCount = path.join(basedir, "test/data/beefy-final-bitfield-0.json")
    const BitFieldFile3SigCount = path.join(basedir, "test/data/beefy-final-bitfield-3.json")
    const ValidatorProofFile0SigCount = path.join(basedir, "test/data/beefy-final-proof-0.json")
    const ValidatorProofFile3SigCount = path.join(basedir, "test/data/beefy-final-proof-3.json")

    const command = process.argv[2]
    const validatorSetID = fixtureData.params.id
    const validatorSetSize =
        process.env["FixedSet"] == "true"
            ? accounts.length
            : process.env["ValidatorSetSize"]
                ? parseInt(process.env["ValidatorSetSize"])
                : 300
    const commitHash = fixtureData.commitmentHash
    let validatorSet: ValidatorSet
    if (process.env["FixedSet"] == "true") {
        validatorSet = new ValidatorSet(
            validatorSetID,
            validatorSetSize,
            accounts.map((account) => account.privateKey)
        )
    } else {
        validatorSet = new ValidatorSet(validatorSetID, validatorSetSize)
    }

    if (command == "GenerateInitialSet") {
        const absentSubsetSize = Math.floor((validatorSetSize - 1) / 3)
        const subsetSize = validatorSetSize - absentSubsetSize
        const randomSet = createRandomSubset(validatorSetSize, subsetSize)
        const participants = randomSet.participants
        const absentees = randomSet.absentees

        const testFixture = {
            validatorSetSize,
            participants,
            absentees,
            validatorRoot: validatorSet.root,
        }
        console.log("Validator Set", testFixture)
        fs.writeFileSync(ValidatorSetFile, JSON.stringify(testFixture, null, 2), "utf8")
        console.log("Beefy fixture writing to dest file: " + ValidatorSetFile)
    } else if (command == "GenerateProofs") {
        generateValidatorProof(BitFieldFile0SigCount, ValidatorProofFile0SigCount, validatorSet, commitHash);
        generateValidatorProof(BitFieldFile3SigCount, ValidatorProofFile3SigCount, validatorSet, commitHash);
    }
}

run()
    .then(() => process.exit(0))
    .catch((error) => {
        console.error(error)
        process.exit(1)
    })
