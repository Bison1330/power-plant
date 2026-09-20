# Vitreus - Layer 0 Blockchain Ecosystem🌿⚡

Vitreus is a  next-generation Layer 0 blockchain platform focused on decentralized energy trading and management, built using Substrate and featuring comprehensive EVM compatibility.

[![Substrate version](https://img.shields.io/badge/Substrate-stable2407-brightgreen?logo=Parity%20Substrate)](https://substrate.io)
[![License](https://img.shields.io/badge/License-GPL%203.0-blue.svg)](LICENSE)

## Overview

Vitreus is a sophisticated blockchain ecosystem designed to revolutionize energy trading and management. Built on Substrate with full EVM compatibility, it combines traditional blockchain capabilities with specialized energy-focused features.

### Core Features

- **Dual-Token System**
    - VTRS (Native Token): Platform governance and staking
    - VNRG (Energy Token): Energy trading and fee payments

- **Energy Trading Infrastructure**
    - Automated Market for energy assets
    - Dynamic exchange rates based on network metrics
    - Warehouse mechanism for supply stability
    - Multi-asset support and path-based routing

- **Advanced Security & Access Control**
    - NFT-based access control system (NAC)
    - Reputation-based validation
    - Tiered VIP system with dynamic privileges
    - Comprehensive slashing mechanisms

- **Economic Features**
    - Dynamic fee calculation system
    - Automated treasury management
    - Flexible vesting schedules
    - Reputation-based rewards

## Architecture

Vitreus consists of several specialized pallets working in harmony:

### Core Pallets

1. **Energy Broker**
    - Decentralized energy trading through AMM
    - Automated price discovery
    - Liquidity provision management

2. **Energy Fee**
    - Dual-token transaction fee mechanism
    - Dynamic fee adjustments
    - Automated token exchanges

3. **Energy Generation**
    - Staking and validation mechanisms
    - Energy-per-stake rate calculation
    - Reputation integration

4. **NAC Managing**
    - NFT-based access control
    - VIPP status management
    - Dynamic level updates

### Supporting Pallets

- **Claiming**: Token distribution and vesting
- **Reputation**: Dynamic scoring system
- **Privileges**: VIP membership management
- **Simple Vesting**: Token lock mechanisms
- **Treasury Extension**: Fund recycling and management
- **Faucet**: Token distribution (it's only supported in testnet)

## Getting Started

### Prerequisites

- Rust 1.74 or later
- `wasm32-unknown-unknown` target
- Node.js (for testing)

### Installation

1. Clone the repository:
```bash
git clone https://github.com/Vitreus-Foundation/power-plant
cd power-plant
```

2. Build the node:
```bash
cargo build --release --features testnet-native
```

3. Run the node:
```bash
./target/release/vitreus-power-plant-node --dev
```

### Development Chain Configuration

The development chain comes with pre-funded accounts for testing:

- **Alith (Sudo)**: `0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac`
- **Baltathar**: `0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0`
- Additional test accounts available in development mode

### Network Configuration

For connecting to the network:
- Chain ID: 1943
- Network Name: vitreus-power-plant
- Currency Symbol: VTRS
- RPC URL: http://localhost:9944/

## Development

### Building for Production

```bash
# Build with mainnet runtime
cargo build --release --features mainnet-native
```

### Reproducing a runtime wasm

To verify that a deployed runtime matches the source, build only the runtime
package at the tagged commit **with the workspace at `/build`** and compare
hashes:

```bash
sudo mkdir -p /build && sudo mount --bind "$PWD" /build   # or clone straight into /build
cd /build && git checkout <commit>
cargo build --release --locked -p vitreus-power-plant-runtime --features testnet-runtime   # or mainnet-runtime
sha256sum target/release/wbuild/vitreus-power-plant-runtime/vitreus_power_plant_testnet_runtime.compact.compressed.wasm
```

Two things make the checkout location matter, and this recipe removes both:

- rustc embeds absolute source paths (panic locations) for every crate, and
  wasm-builder compiles `core`/`alloc` from source, so the rustup home is
  embedded too. `runtime/vitreus/build.rs` passes `--remap-path-prefix` for
  the workspace root, `$CARGO_HOME` and the toolchain sysroot, so those come
  out as `/power-plant/...`, `/cargo/...` and `/rustc-sysroot/...` everywhere.
- cargo hashes each workspace crate's absolute path into its `-C metadata`.
  That changes every symbol hash, and through LLVM's codegen the code and
  data sections with them, so no flag can fix it: the workspace has to sit at
  the same path. `/build` is the convention (it is what `srtool` uses).

What still has to match: the toolchain (`rust-toolchain.toml`), the
dependency set (`--locked`), and the feature. `$CARGO_HOME` and the rustup
home may differ. The `runtime-wasm` job in `.github/workflows/fork-ci.yml`
builds at `/build` and prints the hash for every commit in its job summary,
so a reviewer can compare against CI instead of building.

Runtimes up to spec 225 were built before this; their committed hashes in
`dev-ops/runtimes/` only reproduce from a checkout at `/root/power-plant`.

### Running Tests

```bash
# Run all tests
cargo test --features testnet-native --workspace

# Run specific pallet tests
cargo test -p pallet-energy-broker
```

## Security Considerations

1. **Access Control**
    - Multiple validation layers through NAC system
    - Reputation-based restrictions
    - Tiered privilege system

2. **Economic Security**
    - Dynamic fee mechanisms
    - Slashing for malicious behavior
    - Stake-based validation

3. **Network Stability**
    - Warehouse mechanism for price stability
    - Automated treasury management
    - Progressive rate adjustments

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style

- Follow Rust standard practices
- Use the provided clippy configuration
- Ensure comprehensive test coverage
- Include benchmarks for new features

## Documentation

- [Pallet Documentation](./pallets/ExtrinsicLib.md)

## License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

## Support

- Open an issue for bug reports
- Join our [community](https://discord.gg/vitreus)
- Check technical documentation

## Acknowledgments

Built using:
- [Substrate](https://substrate.io/)
- [Frontier](https://github.com/paritytech/frontier)
