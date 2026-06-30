# Machine-checked verification (Kani)

The fourth assurance leg, above source/binary/silicon: a **machine-checked** proof
of functional correctness and runtime-safety of the AEAD, using the
[Kani](https://github.com/model-checking/kani) bounded model checker (CBMC
backend). Unlike a test, each harness explores *all* inputs in a bounded domain
symbolically — so a passing harness is a proof over that domain, not a sample.

## What is proven

Harnesses live in `rustguard-core/src/kani_proofs.rs` (compiled only under
`cfg(kani)`; never part of a normal build or `cargo test`).

**Safety** (no panic, no integer overflow, no out-of-bounds, no UB) — for a
`#![forbid(unsafe_code)]` crate this upgrades "contains no `unsafe`" to "provably
cannot crash or overflow" across the API:
- `verify_permutation_safety` — the permutation, for any state and any rounds ≤ 12
- `verify_encrypt_safety` — encrypt, any key/nonce, 16-byte message
- `verify_decrypt_safety` — decrypt, any inputs (including the wipe-on-failure path)
- `verify_encrypt_ad_safety` — the associated-data + partial-block padding paths
- `verify_hash_safety` — ASCON-HASH, any 16-byte input

**Functional correctness** (default — fixed key/nonce, symbolic 8-byte message):
- `verify_recovery_msg` — decryption recovers the plaintext for *every* message
  (the AEAD data-path inverse). Tag authentication is pinned exactly by the KAT
  vectors (`tests/kat.rs`) and asserted by the deep harnesses below.

**Deep (feature `kani-deep`, off by default — intractable on commodity hardware):**
- `verify_roundtrip_auth` — a genuine tag authenticates (fixed key/nonce)
- `verify_forgery_rejected` — a wrong tag never authenticates (no forgery)
- `verify_roundtrip_symbolic_key` — round-trip with a fully symbolic key and nonce

These assert *tag authentication*, which forces the solver to equate the genuine
tag through the finalize permutation (~24 rounds) — correct by construction, but
not solvable in reasonable time on a laptop. They are gated off so the default
suite stays fast and reliable, and kept as runnable artifacts for a larger
machine: `cargo kani -p rustguard-core --features kani-deep`.

Message sizes are kept small and concrete (contents symbolic) so the model checker
stays tractable; `unwind(17)` fully unwinds the 12-round permutation and the
16-byte tag comparison. The source is size-generic; these harnesses certify the
representative block boundaries.

## Honest scope

- Kani proves **functional correctness and runtime safety**, not the constant-time
  property — that is covered by the binary census (`ct_binary.py`) and the
  on-silicon dudect measurement. The three legs are complementary.
- Bounded model checking certifies the bounded domain (these message sizes), not
  arbitrary-length inputs; the cipher logic is identical across sizes, so the
  certified boundaries are representative, but this is a bound, stated plainly.
- Under `cfg(kani)` only, `zeroize`'s inline-asm optimization barrier (which Kani
  cannot model) is replaced by a semantically identical plain wipe, and `State`'s
  zeroize-on-drop is omitted. This changes nothing about the values computed or
  the safety properties; **production builds are byte-for-byte unaffected**.

## Running it

```sh
cargo install --locked kani-verifier && cargo kani setup    # one-time
cargo kani -p rustguard-core                                 # default suite
cargo kani -p rustguard-core --harness verify_recovery_msg   # one harness
cargo kani -p rustguard-core --features kani-deep            # + deep harnesses
```

CI runs the default suite on every push via the `proofs` job
(`model-checking/kani-github-action`). The `kani-deep` harnesses are not run in
CI (they do not terminate in CI time); they are kept as runnable artifacts for a
larger machine.

## Results (confirmed, Kani 0.67.0)

Default suite — **6 / 6 verified** on a commodity laptop (WSL2, 5.8 GB):

| Harness | Property | Result | Time |
|---|---|---|---|
| `verify_permutation_safety` | no panic/overflow/UB, any state, rounds ≤ 12 | ✅ SUCCESSFUL | 23 s |
| `verify_encrypt_safety` | encrypt: no panic/overflow/UB, any input | ✅ SUCCESSFUL | 21 s |
| `verify_decrypt_safety` | decrypt: no panic/overflow/UB, any input | ✅ SUCCESSFUL | 50 s |
| `verify_encrypt_ad_safety` | AD + partial-block padding paths | ✅ SUCCESSFUL | 31 s |
| `verify_hash_safety` | ASCON-HASH: no panic/overflow/UB | ✅ SUCCESSFUL | 26 s |
| `verify_recovery_msg` | decryption recovers the plaintext, every message | ✅ SUCCESSFUL | 64 s |

Deep harnesses (`--features kani-deep`) do not terminate on this hardware; they
encode tag authentication / forgery over the finalize permutation, which is
correct by construction but beyond a laptop SAT solver. Authentication is covered
instead by the exact KAT vectors in `rustguard-core/tests/kat.rs`.
