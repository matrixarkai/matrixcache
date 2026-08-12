# Security Policy

## Supported versions

MatrixCache is pre-1.0. Security fixes target the latest `main` and the most recent
published `0.x` release.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately** via GitHub Security Advisories
— use **"Report a vulnerability"** at
<https://github.com/bjmeetsfo/MatrixCache/security/advisories/new> — rather than
opening a public issue or pull request.

Include, if possible:

- a description of the issue and its impact,
- the affected version or commit,
- steps to reproduce or a proof of concept.

We aim to acknowledge reports within a few business days and will coordinate a fix
and disclosure timeline with you.

## Scope

Cache-correctness defects that could cause **data loss, stale data served as fresh,
or corruption** across the DRAM / persistent-memory / SSD tiers — including
RocksDB-backed persistence, eviction, async writeback, and restart refill — are
treated as security issues, not merely bugs.
