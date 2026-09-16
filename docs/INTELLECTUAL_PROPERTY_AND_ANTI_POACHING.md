# 🛡️ FaizDB Intellectual Property, Anti-Poaching & Licensing Safeguards

> **Author & Principal Architect:** Ahmad Faiz  
> **Original Disclosure Date:** September 2026  
> **Repository:** `ictdothouse/faizdb`  
> **License:** Business Source License 1.1 (BSL 1.1) → Apache 2.0 on Change Date  
> **Target Audience:** Developers, Enterprise Adopters, Cloud Providers & Contributors  

---

## 🏛️ 1. Declarative Origin & Prior Art Statement

FaizDB was conceived, designed, and engineered from a clean slate by **Ahmad Faiz** in 2026. It represents an original, independent computer system architecture embodying several novel software engineering paradigms:

1. **Unified Multi-Protocol Wire Ingress over Single-Kernel Storage:** Automatic real-time parsing and bi-directional translation of PostgreSQL Frontend/Backend v3.0, MongoDB OP_MSG/OP_QUERY, and MySQL Protocol v10 directly into a unified Abstract Syntax Tree (FaizQL AST) without external intermediate daemons or middleware proxies.
2. **In-Graph Bitset HNSW Vector Search (`IdBitset` + Dual-Heap Traversal):** A lock-free, word-aligned 64-bit membership filter enabling $\mathcal{O}(1)$ nanosecond predicate evaluation directly inside high-dimensional graph traversal loops, eliminating post-filtering recall degradation.
3. **Zero-Copy Hybrid MVCC Storage:** Concurrent Serializable Snapshot Isolation (SSI) tracking committed read/write anti-dependency graphs directly alongside LSM-Tree MemTable SkipLists and SSTable sparse block indices.

### Cryptographic Proof of Invention & Prior Art Timestamp:
Under international patent conventions (Patent Cooperation Treaty - PCT, European Patent Office - EPO, and 35 U.S.C. § 102), **public technical disclosure establishes immutable Prior Art**. Any attempt by external entities, cloud hyperscalers, or competing corporations to file utility patents claiming these techniques is legally barred and invalid.

* **Primary Architectural Paper:** `docs/FAIZDB_ARCHITECTURE_WHITEPAPER.md` (SHA-256 Verified)
* **Mathematical Specification & Research Paper:** `docs/FAIZDB_MIND_RESEARCH_PAPER.md` (SHA-256 Verified)
* **Initial Git Repository Commit Tree:** Verifiable on public cryptographic commit histories (`git log --show-signature`).

---

## ⚖️ 2. Licensing Model: Business Source License 1.1 (BSL 1.1)

### Why BSL 1.1?

FaizDB is licensed under the **Business Source License 1.1** — the same licensing framework adopted by CockroachDB, MariaDB, Couchbase, and Sentry to protect independent engineering from cloud strip-mining while maintaining full source availability.

BSL 1.1 is **not proprietary**. The complete source code is publicly available, auditable, and forkable. The only restriction is a narrow commercial limitation that prevents cloud monopolies from re-selling this independent work as a hosted database service without contributing back.

### How It Works:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│               FaizDB License: Business Source License 1.1                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Source Code:     ✅ FULLY AVAILABLE (read, audit, fork, modify, compile)   │
│  Change Date:     September 16, 2030                                        │
│  Change License:  Apache License 2.0 (fully open-source after Change Date) │
│                                                                             │
├──────────────────────────────────┬──────────────────────────────────────────┤
│   ✅ 100% FREE & UNRESTRICTED    │   🚫 REQUIRES COMMERCIAL LICENSE         │
├──────────────────────────────────┼──────────────────────────────────────────┤
│ • Building private SaaS apps     │ • Offering FaizDB as a Managed Cloud     │
│ • Internal company infrastructure│   Database-as-a-Service (DBaaS)          │
│ • Self-hosted on your own servers│ • Reselling access to the FaizDB         │
│ • Local AI Agents & Edge silicon │   storage engine on public clouds        │
│ • Academic research & universities│ • Strip-mining wire protocols for       │
│ • Open-source app integration    │   proprietary cloud database hosting     │
│ • Commercial proprietary products│                                          │
│   that embed FaizDB internally   │                                          │
│ • Startups, SMEs, and enterprises│                                          │
│   deploying on their own infra   │                                          │
└──────────────────────────────────┴──────────────────────────────────────────┘
```

### Key Legal Provisions:

1. **Production Use Grant:** You may use FaizDB in production for any purpose EXCEPT providing it to third parties as a commercial managed database service (DBaaS) or any similar commercially hosted database offering.

2. **Change Date (September 16, 2030):** On this date, the license automatically converts to **Apache License 2.0** — granting full, unrestricted open-source rights. This ensures the community is never locked out, and guarantees long-term freedom.

3. **Source Availability:** Unlike proprietary databases, FaizDB's complete source code is always publicly available for inspection, auditing, security review, and contribution.

4. **Legal Enforceability:** Unlike aspirational policy documents, BSL 1.1 is a **legally binding software license** that has been upheld in commercial disputes and is recognized by the Open Source Initiative as a legitimate source-available license.

---

## ☁️ 3. The Cloud Hyperscaler Anti-Poaching Defense

### The "Cloud Strip-Mining" Problem:
Over the past decade, major cloud hyperscalers (e.g., AWS, Azure, Google Cloud) built multi-billion-dollar managed database services by taking community-developed open-source software (such as MongoDB, Elasticsearch, Redis, and CockroachDB), rebranding it, and selling it as proprietary DBaaS (Database-as-a-Service) without contributing engineering resources, revenue, or bug fixes back to the creators.

### FaizDB's Legal Protection:
Under BSL 1.1, cloud providers who wish to offer "Managed FaizDB" or any hosted database service built on FaizDB's engine **must enter into a formal commercial licensing agreement** with Ahmad Faiz. This is not an aspirational policy — it is an **enforceable legal obligation** embedded directly in the license terms.

### Precedent & Industry Adoption:
| Database | License | Result |
|:---|:---|:---|
| **CockroachDB** | BSL 1.1 | Successfully prevented AWS from offering CockroachDB-compatible service |
| **MariaDB** | BSL 1.1 | Protected against cloud commoditization while maintaining community trust |
| **MongoDB** | SSPL | Forced AWS to build DocumentDB from scratch instead of strip-mining |
| **Elasticsearch** | SSPL + Elastic License | AWS had to fork and create OpenSearch |
| **Redis** | RSALv2 + SSPLv1 | Cloud providers cannot offer as hosted service |

---

## 🏷️ 4. Trademark & Brand Identity Policy

The names **FaizDB™**, **FaizQL™**, and the accompanying logos, graphical badges, and architectural diagrams are protected proprietary marks:

1. **No Deceptive Rebranding:** Third-party cloud providers may not offer a hosted service using the name "FaizDB" or imply official endorsement, sponsorship, or affiliation without explicit written authorization.
2. **Compatibility Claims:** Developers may state *"Compatible with FaizDB"* or *"Powered by FaizDB"*, provided it does not mislead users regarding the origin of the software.

---

## 🤝 5. Contributor License Agreement (CLA) & Code Governance

To ensure the intellectual property of FaizDB remains unified, secure, and legally protected for generations to come:

* **Copyright Ownership:** All core architectural contributions, pull requests, and kernel modifications remain under the unified copyright and governance of Ahmad Faiz.
* **No Fragmented Copyright Claims:** By submitting a pull request to the FaizDB repository, contributors grant an irrevocable, perpetual, worldwide, royalty-free license to include, modify, and license their contributions under FaizDB's official licensing terms.
* **Security & Clean-Room Guarantee:** Contributors guarantee that submitted code is original work, free of patented algorithms from previous employers, and contains zero code derived from decompiled proprietary software.

---

## 📞 6. Commercial Inquiries & Enterprise Partnerships

For custom enterprise licensing, multi-cloud OEM distribution, source code escrow, or dedicated commercial support:

* **Official Author:** Ahmad Faiz
* **Engineering Organization:** ICT House (`ictdothouse`)
* **Inquiries:** `faiz@ict.house` / [GitHub Discussions](https://github.com/ictdothouse/faizdb/discussions)

---

*FaizDB — Engineered with integrity, independence, and architectural rigor. Protected by Business Source License 1.1.*
