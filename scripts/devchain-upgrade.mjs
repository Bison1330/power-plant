// Sudo-upgrade the DEV chain's runtime and print what changed. Dev-chain
// only: signs with the well-known Alith key, which is the dev chain's sudo.
//
//   node scripts/devchain-upgrade.mjs path/to/vitreus_power_plant_testnet_runtime.compact.compressed.wasm
//   RPC=ws://127.0.0.1:9945     # must be the node's own port: the public /rpc
//                               # proxy caps frames at 64 KiB and setCode is ~2.3 MB
//
// Refuses to downgrade or re-apply the same spec. After the upgrade it
// reconnects and reports spec_version, the DEX storage version, and whether
// the metadata still carries the storage items removed in b0621d6.
import { readFileSync } from 'node:fs';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { u8aToHex } from '@polkadot/util';

const RPC = process.env.RPC ?? 'ws://127.0.0.1:9945';
const wasmPath = process.argv[2];
if (!wasmPath) { console.error('usage: node scripts/devchain-upgrade.mjs <runtime.compact.compressed.wasm>'); process.exit(2); }

await cryptoWaitReady();
const alith = new Keyring({ type: 'ethereum' }).addFromUri('0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133');
let api = await ApiPromise.create({ provider: new WsProvider(RPC, 2000), noInitWarn: true });

const before = api.runtimeVersion.specVersion.toNumber();
const code = readFileSync(wasmPath);
console.log(`chain ${api.runtimeChain} · on-chain spec ${before} · sudo ${alith.address} · wasm ${wasmPath} (${code.length} bytes)`);

const sudoKey = (await api.query.sudo.key()).toString();
if (sudoKey.toLowerCase() !== alith.address.toLowerCase()) { console.error(`sudo key on chain is ${sudoKey}, not Alith; refusing`); process.exit(1); }

const dexVersionBefore = (await api.query.vitreusDex.palletVersion()).toNumber();
console.log(`vitreusDex storage version before: ${dexVersionBefore}`);

const tx = api.tx.sudo.sudoUncheckedWeight(api.tx.system.setCode(u8aToHex(code)), { refTime: 0, proofSize: 0 });
await new Promise((resolve, reject) => {
  tx.signAndSend(alith, { nonce: -1 }, (r) => {
    if (r.dispatchError) reject(new Error(`dispatch error: ${r.dispatchError.toString()}`));
    if (r.status.isInBlock) {
      const evs = r.events.map((e) => `${e.event.section}.${e.event.method}`);
      console.log(`setCode in block ${r.status.asInBlock.toHex()} · events: ${evs.filter((e) => /system\.|sudo\./.test(e)).join(', ')}`);
      if (!evs.includes('system.CodeUpdated')) reject(new Error('no system.CodeUpdated event — the upgrade did not apply'));
      resolve();
    }
    if (r.isError) reject(new Error(`tx ${r.status.type}`));
  }).catch(reject);
});

// The new runtime is live from the NEXT block. Reconnect so the client picks
// up the new metadata rather than the cached one.
await new Promise((r) => setTimeout(r, 12000));
await api.disconnect();
api = await ApiPromise.create({ provider: new WsProvider(RPC, 2000), noInitWarn: true });
const after = api.runtimeVersion.specVersion.toNumber();
const dexVersionAfter = (await api.query.vitreusDex.palletVersion()).toNumber();
const dexStorage = api.runtimeMetadata.asLatest.pallets.find((p) => p.name.toString() === 'VitreusDex').storage.unwrap().items.map((i) => i.name.toString());
console.log(`after: spec ${after} (was ${before}) · vitreusDex storage version ${dexVersionAfter} (was ${dexVersionBefore})`);
console.log(`vitreusDex storage items: ${dexStorage.join(', ')}`);
console.log(`TotalEnergySold / TotalEnergyBurned present: ${dexStorage.includes('TotalEnergySold') || dexStorage.includes('TotalEnergyBurned')} (expected false from 219)`);
console.log(`launchpad pallet present: ${!!api.tx.launchpad}`);
await api.disconnect();
process.exit(after > before ? 0 : 1);
