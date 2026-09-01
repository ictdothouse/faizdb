# Security Policy

## Supported Versions

| Version | Supported |
|:--------|:----------|
| `0.1.x` (main) | ✅ Actively supported |

## Reporting a Vulnerability

**Please DO NOT open a public GitHub Issue for security vulnerabilities.**

Report security issues via **private email** to:

📧 **faiz@ict.house**

Include in your report:
- A clear description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact

### Response Timeline

| Step | Target |
|:-----|:-------|
| Initial acknowledgement | Within **48 hours** |
| Severity assessment | Within **5 business days** |
| Patch release (Critical/High) | Within **14 days** |
| Public disclosure | After patch is released |

We follow [Responsible Disclosure](https://en.wikipedia.org/wiki/Coordinated_vulnerability_disclosure).

## Security Architecture

FaizDB implements the following security measures:

- **Password Hashing:** Argon2id with random salt (industry standard for 2026)
- **JWT Authentication:** EdDSA (Ed25519) — asymmetric, immune to timing attacks
- **Encryption at Rest:** AES-256-GCM via `ring` (FIPS 140-2 compliant primitives)
- **Transport Security:** TLS 1.3 via `rustls` (no legacy TLS 1.0/1.1)
- **Memory Safety:** Written in Safe Rust — no `unsafe` code in core engine
- **Rate Limiting:** Built-in brute-force protection with automatic IP banning
- **RBAC:** Three-tier role system (Admin, ReadWrite, ReadOnly)
- **Audit Logging:** JSON-Lines security event log for compliance
