| primitive | cond branches | IT blocks | variable-latency div | measured | screening outcome |
|---|---|---|---|---|---|
| CANARY-control | 6 | 0 | 0 | leaks | true positive |
| aes-gcm | 5 | 1 | 0 | cycle-invariant | false positive |
| aes-gcm-siv | 2 | 1 | 0 | cycle-invariant | false positive |
| ascon-aead | 0 | 1 | 0 | cycle-invariant | true negative |
| ccm-aes128 | 6 | 5 | 0 | cycle-invariant | false positive |
| chacha20poly1305 | 45 | 1 | 0 | cycle-invariant | false positive |
| cmac-aes128 | 3 | 1 | 0 | cycle-invariant | false positive |
| eax-aes128 | 4 | 8 | 0 | cycle-invariant | false positive |
| hmac-sha256 | 6 | 1 | 0 | cycle-invariant | false positive |
| rustguard-LEAKY-control | 2 | 0 | 0 | leaks | true positive |
| rustguard-ascon128 | 2 | 1 | 0 | cycle-invariant | false positive |
