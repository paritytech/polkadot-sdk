# Subnet Pallet

## Overview

This Substrate/FRAME pallet provides a framework for managing subnets with resource configurations and provider management. It allows for the creation and tracking of subnets with specific computational requirements.

## Key Features

- Create subnets with defined resource configurations
- Track subnet metrics and performance
- Manage providers within each subnet
- Integrate with a king node authentication system

## Main Components

### ResourceConfig
Defines minimum computational requirements for a subnet:
- Computational Capacity
- Memory
- Storage
- Bandwidth

### SubnetInfo
Contains detailed information about each subnet:
- Subnet King (owner)
- Resource Configuration
- List of Providers

### SubnetMetrics
Tracks subnet performance:
- Total Resources
- Active Providers
- Total Rewards Distributed

## Usage

To create a subnet, a user must:
1. Be authenticated as a king node
2. Provide a resource configuration
3. Generate a unique subnet ID

## Events

- `SubnetCreated`: Emitted when a new subnet is formed
- `ResourcesUpdated`: Signals changes in subnet resources
- `MetricsUpdated`: Indicates updates to subnet performance metrics

## Errors

- `SubnetNotFound`: Subnet does not exist
- `ResourceRequirementsNotMet`: Subnet fails to meet minimum resource criteria
- `MaxProvidersReached`: Cannot add more providers to the subnet
- `UnauthorizedKing`: Subnet creation attempted by unauthorized account

## Dependencies

- Requires `pallet_king` for king node authentication
- Uses Substrate's FRAME support libraries

## Development

This pallet is developed with `dev_mode` enabled, suitable for testing and iterative development.