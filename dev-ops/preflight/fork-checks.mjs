// Read-only checks against a chopsticks fork (port 8000) and, for the diff,
// the live chain it forked. Prints: runtime version on the fork, whether the
// first built blocks run (that is where on_runtime_upgrade executes), the
// DEX storage version, and the pallet-level diff live → fork.
//   TESTNET_WS=wss://... node dev-ops/preflight/fork-checks.mjs
import { ApiPromise, WsProvider } from '@polkadot/api';
const FORK = process.env.FORK_WS ?? 'ws://127.0.0.1:8000';
const LIVE = process.env.TESTNET_WS;
const forkProvider = new WsProvider(FORK, 2000);
const fork = await ApiPromise.create({ provider: forkProvider, noInitWarn: true });
console.log(`fork: ${fork.runtimeChain} spec ${fork.runtimeVersion.specVersion} (${fork.runtimeVersion.specName})`);
const head0 = (await fork.rpc.chain.getHeader()).number.toNumber();
const lastUpgrade = (await fork.query.system.lastRuntimeUpgrade()).toJSON();
console.log(`head ${head0} · System.LastRuntimeUpgrade before building: ${JSON.stringify(lastUpgrade)}`);
// Build three empty blocks: the first one runs Executive::on_runtime_upgrade
// (LastRuntimeUpgrade 213 -> 219) i.e. the Unreleased migration tuple.
for (let i = 0; i < 3; i++) {
  const r = await forkProvider.send('dev_newBlock', [{ count: 1 }]).catch((e) => ({ error: String(e) }));
  const head = (await fork.rpc.chain.getHeader()).number.toNumber();
  const events = await fork.query.system.events();
  const names = events.map((e) => `${e.event.section}.${e.event.method}`);
  console.log(`built block ${head} · ${names.length} events: ${names.filter((n) => !/^(system\.ExtrinsicSuccess|timestamp\.)/.test(n)).join(', ') || '(only the inherents)'}`);
  if (r?.error) console.log('  dev_newBlock error:', r.error);
}
console.log(`System.LastRuntimeUpgrade after: ${JSON.stringify((await fork.query.system.lastRuntimeUpgrade()).toJSON())}`);
console.log(`vitreusDex storage version: ${(await fork.query.vitreusDex.palletVersion()).toNumber()} · launchpad storage version: ${(await fork.query.launchpad.palletVersion()).toNumber()}`);
console.log(`launchpad params: ${JSON.stringify((await fork.query.launchpad.params()).toHuman())}`);
const pallets = (api) => new Map(api.runtimeMetadata.asLatest.pallets.map((p) => [p.name.toString(), { index: p.index.toNumber(), calls: p.calls.isSome ? api.registry.lookup.getSiType(p.calls.unwrap().type).def.asVariant.variants.map((v) => v.name.toString()).sort() : [], storage: p.storage.isSome ? p.storage.unwrap().items.map((i) => i.name.toString()).sort() : [] }]));
const pf = pallets(fork);
if (LIVE) {
  const live = await ApiPromise.create({ provider: new WsProvider(LIVE, 2000), noInitWarn: true });
  console.log(`live: ${live.runtimeChain} spec ${live.runtimeVersion.specVersion} · sudo key: ${live.query.sudo ? (await live.query.sudo.key()).toString() : 'no sudo pallet'}`);
  const pl = pallets(live);
  const added = [...pf.keys()].filter((n) => !pl.has(n));
  const removed = [...pl.keys()].filter((n) => !pf.has(n));
  const changed = [...pf.keys()].filter((n) => pl.has(n) && JSON.stringify(pf.get(n)) !== JSON.stringify(pl.get(n)));
  console.log(`pallet diff live -> fork: added ${JSON.stringify(added.map((n) => `${n}@${pf.get(n).index}`))} · removed ${JSON.stringify(removed)} · changed ${JSON.stringify(changed)}`);
  for (const n of changed) {
    const a = pl.get(n), b = pf.get(n);
    console.log(`  ${n}: calls +${JSON.stringify(b.calls.filter((c) => !a.calls.includes(c)))} -${JSON.stringify(a.calls.filter((c) => !b.calls.includes(c)))} · storage +${JSON.stringify(b.storage.filter((c) => !a.storage.includes(c)))} -${JSON.stringify(a.storage.filter((c) => !b.storage.includes(c)))}`);
  }
  await live.disconnect();
}
await fork.disconnect();
process.exit(0);
