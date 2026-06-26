# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.2.x | ✓ |

## Reporting a Vulnerability

If you discover a security vulnerability in RustGuard, please report it
responsibly by emailing **ta08451@st.habib.edu.pk** with:

- A description of the vulnerability
- Steps to reproduce
- Potential impact assessment

Do **not** open a public GitHub issue for security vulnerabilities.

We aim to acknowledge receipt within 48 hours and provide a fix or mitigation
within 14 days for critical issues.

## Scope

RustGuard implements ASCON-128 (NIST IR 8454) as the device under test for a
study of constant-time behavior on embedded silicon. Reported vulnerabilities
may relate to:

- Incorrect implementation of the ASCON specification (the KATs in
  `rustguard-core/tests/kat.rs` are the reference oracle)
- Timing or power side-channel leakage that survives compilation to hardware
- Memory unsafety (note: `#![forbid(unsafe_code)]` is enforced on the crypto
  crates; the only `unsafe` is isolated MMIO in the firmware crates)
- Nonce-reuse vulnerabilities in the PAP protocol design (see
  `docs/protocol_security.md`)
- Authentication bypass in the tag comparison logic

Note: the `tvla-leaky-control` feature compiles a deliberately variable-time tag
check. It is a research positive control and is **not** a vulnerability — it must
never be enabled in a production build.

Out of scope: vulnerabilities in upstream dependencies (`subtle`, `zeroize`,
`heapless`). Please report those to the respective crate maintainers.

## Cryptographic Disclaimer

This library is provided for research and educational purposes. It has not
undergone a formal third-party security audit. Use in production deployments
requires independent review.
