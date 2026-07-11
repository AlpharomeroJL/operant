---
name: operant-contracts
description: Schema-first rules for any packet touching a contract surface.
---
Contracts are append-only during the campaign: add optional fields, never rename or
remove. Breaking need: ADR, schema version bump, fixtures in both versions. Every
cross-lane function round-trips a fixture in a test. Code vs fixture disagreement:
the fixture wins until an ADR says otherwise. The microcopy glossary is a contract.
