**Force-pushed with two security fixes — please re-review from `8fddf1e`.**

Both were found live on our dev chain after this PR was opened, and one is a
denial of service, so we pushed the fix rather than leave a reviewer reading
code we know is exploitable. `bench/repairs-before`-style: the old tree is
still at the previous SHA if you want the diff.

**FM-17 — asset-id squatting (live DoS).** This was the launchpad's "FM-14";
we renumbered it here to end a number collision with vitreus-dex
`SECURITY_AUDIT.md` **Finding 14** (a different finding, below), which was the
whole reason it kept getting confused. Anyone can `pallet_assets::create` the
reserved id `LaunchAssetBase + NextLaunchId` for one asset deposit and block
**every** `create_launch` with `AssetIdTaken` until a runtime migration bumps
the counter — a whole pad bricked for the price of nothing, and we saw it on
the live chain where the next slot was already taken. Fix: `create_launch`
walks a monotonic `NextAssetId` cursor to the first free id (bounded by
`MAX_ASSET_ID_SCAN`); a squatter only pushes the cursor forward at a deposit
per id and creation always succeeds. **No migration** — the cursor defaults to
`LaunchAssetBase + NextLaunchId`, so a chain with a slot already squatted just
skips it on the next create. Test `fm17_asset_id_squatting_is_skipped`.

**Finding 14 — a routed fee slice below ED.** A routed protocol/creator/
treasury slice below the native ED could not create a recipient that did not
exist yet (the DEX fee escrow, the treasury vault before funding), so the
whole swap or curve buy failed with an unreadable `Token(BelowMinimum)`.
`do_swap` now leaves a sub-ED slice in the pool — it accrues to LPs at the next
`sync_reserves`, the same way the pool's own fee share does (D7) — when its
recipient has no account, and the launchpad folds a sub-ED curve treasury
slice into the protocol share. **One caveat we made explicit in the code and
the DEX audit:** a fee-routing total (`ProtocolFeesUnclaimed`, the sink's
tally, `treasury_fees_paid`) is therefore *not exact* by a sub-ED amount
redirected this way; the window is only "before the recipient's first ≥ED
credit". A DEX `GenesisConfig` funds the fee escrow at genesis; the consumer's
runtime carries the equivalent upgrade-path migration (it does not belong in
this pallet crate). Test `finding14_genesis_funds_the_fee_escrow`; the
integration red→green lives in the treasury PR that stacks on this.

`LAUNCHPAD_SPEC`, `REVIEW_2026-09-17.md` and the DEX `SECURITY_AUDIT.md` now
cross-reference FM-17 and Finding 14 so they cannot be conflated again. CI is
green (44 launchpad, 108 dex, benchmarks, fmt, clippy).
