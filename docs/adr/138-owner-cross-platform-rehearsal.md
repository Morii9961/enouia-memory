# ADR-MEM-138 — Synthetic Windows-to-Linux Owner restore rehearsal

2026-11-11. Morii approved merging ADR136/137 and continuing the existing route:
build and accept locally, then migrate to Linux. Native round trips on each OS do
not yet prove that an export produced on Windows restores on Linux.

## Decision

Add one example harness with `export` and `restore <synthetic-package>` modes.
Export accepts no input path: it uses the existing built-in synthetic formation
suite in a newly allocated temporary root, forms records, logically deletes one,
and saves the native snapshot, ledger, visible records, receipts and source
evidence as expectations beside an ADR136 export. Move the source out of its old
location and retain all generated material. No cleanup command.

Copy only the export and expectations to the other OS. Restore verifies the
package and the synthetic descriptor, allocates its own empty temporary target,
checks the normal-open quarantine, explicitly activates this fresh synthetic copy,
then compares native observations and initialization receipts with the producer.
No new formation commit, repeated work, real model or source root may be needed.

Use the same harness for an on-Windows round trip and the actual Windows-export
to Linux-restore drill. Save counts and validation results in the public report;
generated roots and package paths stay outside Git. Production APIs, Runtime
surface, schemas and dependencies do not change.

## Limits

This is synthetic migration evidence, not real Owner enrollment, independent
encrypted-media recovery, primary-authority transfer, VPS deployment or ChatGPT
acceptance. Real initialization still needs Morii's explicit start authorization
and chosen durable directory. Long-term read policies and deployment remain
separate owner decisions. No new MV stage starts.
