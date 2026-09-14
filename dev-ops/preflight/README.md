# Pre-flight against the live testnet (read-only)

**Testnet RPC:** `wss://rpc.testnet.compliq.io:9945` — from the vitreus.io wallet
guide, not the chain spec (which only carries a p2p bootnode). Spec 213,
transaction_version 4, 74 pallets, Faucet at 240; indices 43 (VitreusDex) and
57 (Launchpad) are free there.

1. `TESTNET_WS=wss://rpc.testnet.compliq.io:9945 npx @acala-network/chopsticks@1.5.1 --config dev-ops/preflight/chopsticks-testnet.yml`
   forks testnet at head with our runtime as a wasm override and funds the dev
   keys. Nothing is sent to testnet.
2. `TESTNET_WS=... node dev-ops/preflight/fork-checks.mjs` — first blocks (where
   on_runtime_upgrade runs), storage versions, pallet diff live → fork.
3. `RPC=ws://127.0.0.1:8000 node scripts/devchain-launchpad.mjs` — a launch end
   to end on forked state.
4. Frontend e2e against the fork: `VITREUS_RPC_URL=ws://127.0.0.1:8000
   NEXT_PUBLIC_VITREUS_WS_URL=ws://localhost:8000 npx next dev -p 3005`, then
   `scripts/e2e-swap.mjs` / `scripts/e2e-liquidity.mjs` on the graduated pool.
