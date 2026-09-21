**Force-pushed (rebased onto the updated base PR).**

This stacks on the dex+launchpad PR, which was just force-pushed with two
security fixes (FM-17 asset-id squatting — a live DoS — and Finding 14, the
sub-ED routed fee); see that PR's comment. Rebased onto its new tip so the
diff stays clean.

The treasury pallet's own logic is unchanged. What moved here is the **test
coverage** for those fixes and the fuzz harness:

- `finding14_a_sub_ed_routed_slice_does_not_fail_the_swap` — the integration
  red→green for Finding 14, run through the real DEX at a realistic ED (10^12,
  where the bug actually bites; the DEX mock's ED is 1).
- The proptest fuzz harness with **Finding 14's allowlist entry deleted**: a
  sub-ED routed slice failing a trade was allowed "by name" while the finding
  was open, and is now a hard failure — the check the fix leaves behind. 5000
  cases stay green.
- The harness now also runs on twenty-byte accounts (the D8 collision class)
  with a fee layer that buys VNRG through the broker when a caller is short,
  and dumps its op corpus to JSON for the consumer's phase-two replay harness
  (the same sequences, replayed against a chopsticks fork of the live chain).

CI green (40 treasury incl. fuzz, 44 launchpad, 108 dex, benchmarks,
try-runtime, fmt, clippy).
