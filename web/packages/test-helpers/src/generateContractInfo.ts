import fs from "fs"
import path from "path"

const run = async () => {
    const NetworkId = process.env.ETH_NETWORK_ID || 15
    const basedir = process.env.contract_dir || "../contracts"
    const DeployInfoFile = path.join(
        basedir,
        `./broadcast/DeployScript.sol/${NetworkId}/run-latest.json`
    )
    const BuildInfoDir = path.join(basedir, "./out")
    const DestFile =
        process.argv.length >= 3 ? process.argv[2] : process.env["output_dir"] + "/contracts.json"
    type Contract = {
        [key: string]: ContractInfo
    }
    let contracts: Contract = {}
    const deploymentInfo = JSON.parse(fs.readFileSync(DeployInfoFile, "utf8"))
    type ContractInfo = {
        abi?: object
        address?: string
    }
    for (let transaction of deploymentInfo.transactions) {
        if (transaction.transactionType === "CREATE") {
            let contractName: string = transaction.contractName
            if (contractName) {
                let contractInfo: ContractInfo = { address: transaction.contractAddress }
                let contractBuildingInfo = JSON.parse(
                    fs.readFileSync(
                        path.join(BuildInfoDir, contractName + ".sol", contractName + ".json"),
                        "utf8"
                    )
                )
                contractInfo.abi = contractBuildingInfo.abi
                contracts[contractName] = contractInfo
            }
        }
    }
    fs.writeFileSync(DestFile, JSON.stringify({ contracts }, null, 2), "utf8")
}

run()
    .then(() => {
        console.log("Contract File generated successfully")
        process.exit(0)
    })
    .catch((err) => {
        console.error(err)
    })
