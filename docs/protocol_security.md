# RustGuard-PAP security argument (revised protocol)

This note states the security goals of the revised Packet Authentication
Protocol and argues informally why the construction meets them. It is written to
support the paper's protocol section, not to replace a formal proof.

## Nonce construction

```
nonce (128 bit) = epoch (32) ‖ counter (64) ‖ uid_hash (32)
```

* `epoch`    — persisted to NVM, incremented once per boot before any send.
* `counter`  — 64-bit, monotonic within an epoch, starts at 0 each boot.
* `uid_hash` — first 32 bits of ASCON-HASH(device_id), constant per device.

## Goal 1 — nonce uniqueness under reboots (the property the old design lacked)

**Threat.** A constrained MCU reboots uncleanly (brown-out, watchdog, power
glitch). RAM — including a RAM-resident counter — is lost. If the device resumes
sending with a counter value it has used before under the same key, ASCON sees a
repeated (key, nonce) pair. For a sponge AEAD this is catastrophic: the keystream
for the colliding blocks is identical, so XORing two ciphertexts cancels the
keystream and leaks plaintext; with known plaintext it also enables forgery.

**Why the revision prevents it.** The epoch is read from NVM and incremented
*before the first packet of a boot*. Two distinct boots therefore use distinct
epoch prefixes. Within a boot, the 64-bit counter is monotonic. Hence the
(epoch, counter) pair — and therefore the full nonce — never repeats across the
device's lifetime, provided:

1. `store_epoch` durably commits before the first `build_packet` call, and
2. the epoch does not wrap (2^32 boots; at 100 boots/day that is >100,000 years).

Persisting only a 4-byte epoch once per boot (versus the full counter on every
packet) is what makes this affordable on flash-endurance-limited parts: one write
per power cycle instead of one per message.

**Residual assumption.** Durable epoch commit. If NVM write is interrupted by the
same brown-out, the device may reuse an epoch. The firmware mitigates with a
two-slot (A/B) epoch record and a validity flag so a torn write is detectable;
see `docs/hardware_setup.md`. This assumption is stated explicitly in the paper's
limitations.

## Goal 2 — authenticity of metadata

`epoch` and `counter` are placed in the associated data (`AD = header ‖ epoch ‖
seq`) and thus authenticated by the ASCON tag. An adversary cannot alter the
epoch or sequence on the wire without causing tag verification to fail
(forgery probability ≤ 2^-128 per query, inherited from ASCON-128).

## Goal 3 — replay resistance

The receiver maintains `(last_epoch, last_seq)` and accepts a packet only if
`epoch > last_epoch` or (`epoch == last_epoch` and `seq > last_seq`). Any replay
of a previously accepted packet fails this strict-ordering test and is dropped
before decryption. A fresh forgery within the window still requires defeating the
tag (≤ 2^-128).

Note the ordering of checks in `unwrap_packet`: the cheap replay test runs first
to discard obvious replays without spending a permutation, but acceptance updates
the replay window only *after* successful authentication, so an attacker cannot
advance the receiver's window with an unauthenticated high epoch.

## Out of scope

Confidentiality against chosen-ciphertext attack (inherited from ASCON-128),
denial of service, fault injection, and the physical side-channel question are
addressed elsewhere — the side-channel resistance of the *implementation* is the
subject of the main experiment, not of this protocol argument.
