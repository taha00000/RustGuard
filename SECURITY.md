# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | ✓ |

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

This library implements ASCON-128 as specified in NIST IR 8454. Reported
vulnerabilities may relate to:

- Incorrect implementation of the ASCON specification
- Timing side-channel leakage in the Rust source
- Memory unsafety (note: `#![forbid(unsafe_code)]` is enforced at compile time)
- Nonce-reuse vulnerabilities in the PAP protocol design
- Authentication bypass in the tag comparison logic

Out of scope: vulnerabilities in upstream dependencies (`subtle`, `zeroize`,
`heapless`). Please report those to the respective crate maintainers.

## Cryptographic Disclaimer

This library is provided for research and educational purposes. It has not
undergone a formal third-party security audit. Use in production deployments
requires independent review.
