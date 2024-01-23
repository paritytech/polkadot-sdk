// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {Gateway} from "../../src/Gateway.sol";
import {ParaID, OperatingMode} from "../../src/Types.sol";
import {CoreStorage} from "../../src/storage/CoreStorage.sol";
import {Verification} from "../../src/Verification.sol";

import {UD60x18} from "prb/math/src/UD60x18.sol";

contract GatewayMock is Gateway {
    bool public commitmentsAreVerified;

    constructor(
        address beefyClient,
        address agentExecutor,
        ParaID bridgeHubParaID,
        bytes32 bridgeHubHubAgentID,
        uint8 foreignTokenDecimals
    ) Gateway(beefyClient, agentExecutor, bridgeHubParaID, bridgeHubHubAgentID, foreignTokenDecimals) {}

    function agentExecutePublic(bytes calldata params) external {
        this.agentExecute(params);
    }

    function createAgentPublic(bytes calldata params) external {
        this.createAgent(params);
    }

    function upgradePublic(bytes calldata params) external {
        this.upgrade(params);
    }

    function createChannelPublic(bytes calldata params) external {
        this.createChannel(params);
    }

    function updateChannelPublic(bytes calldata params) external {
        this.updateChannel(params);
    }

    function setOperatingModePublic(bytes calldata params) external {
        this.setOperatingMode(params);
    }

    function transferNativeFromAgentPublic(bytes calldata params) external {
        this.transferNativeFromAgent(params);
    }

    function setCommitmentsAreVerified(bool value) external {
        commitmentsAreVerified = value;
    }

    function _verifyCommitment(bytes32 commitment, Verification.Proof calldata proof)
        internal
        view
        override
        returns (bool)
    {
        if (BEEFY_CLIENT != address(0)) {
            return super._verifyCommitment(commitment, proof);
        } else {
            // for unit tests, verification is set with commitmentsAreVerified
            return commitmentsAreVerified;
        }
    }

    function setTokenTransferFeesPublic(bytes calldata params) external {
        this.setTokenTransferFees(params);
    }

    function setPricingParametersPublic(bytes calldata params) external {
        this.setPricingParameters(params);
    }
}

library AdditionalStorage {
    struct Layout {
        uint256 value;
    }

    bytes32 internal constant SLOT = keccak256("org.snowbridge.storage.additionalStorage");

    function layout() internal pure returns (Layout storage sp) {
        bytes32 slot = SLOT;
        assembly {
            sp.slot := slot
        }
    }
}

// Used to test upgrades.
contract GatewayV2 {
    // Reinitialize gateway with some additional storage fields
    function initialize(bytes memory params) external {
        AdditionalStorage.Layout storage $ = AdditionalStorage.layout();

        uint256 value = abi.decode(params, (uint256));

        if (value == 666) {
            revert("initialize failed");
        }

        $.value = value;
    }

    function getValue() external view returns (uint256) {
        return AdditionalStorage.layout().value;
    }
}
