# ADR-MEM-134 — Linux Owner bootstrap

2026-11-09. Fifth slice of the ChatGPT connector route. The remote service
(ADR-MEM-133) serves an initialized Owner Vault, but the native Owner
bootstrap (ADR-MEM-110) that creates one was Windows-only. A Linux VPS could
not create its Vault.

## Decision

Enable `owner_bootstrap` and the `enouia-memory-owner-bootstrap` binary on
Linux, keeping the same commands (`prepare`, `commit`, `recover`, `status`),
journal, recovery and metadata publication.

- **Linux account.** The account is the effective uid from `/proc/self/status`
  bound to this machine's `/etc/machine-id`: the digest of
  `linux-effective-uid:{uid}:machine-id:{id}`. The same uid on another machine
  is a different account. A missing or malformed machine id refuses. The
  effective uid is re-read before every write, where Windows re-checks its
  process session. The registration origin is the same native local CLI
  owner-self origin.
- **Owner-only files.** POSIX files follow the caller's umask, while Windows
  files inherit the owner-only ACL. On Linux the journal is restricted to 0600
  after every write, and the replacement owner-data metadata file is
  restricted before it replaces the original. The existing checks stay
  unchanged: a journal with group or other access is refused, as is a metadata
  file the owner left readable by others. The owner prepares the home with
  `chmod 700` on the directory and `chmod 600` on its marker files.
- **Unchanged.** The native CLI still refuses homes under the temp directory
  and homes whose `storage_class` is not `owner_data`. The created Vault root
  is 0700 on a durable local filesystem (ADR-MEM-133).

## Not in this slice

Migrating the existing Windows Vault or its data to Linux, deployment, the real
exposure scope, and write or capture tools.

## Consequences

The owner bootstrap's unit suite now runs on Linux; the test of Windows
process evidence stays Windows-only. Evidence:
[validation](../validation/Linux-owner-bootstrap.md).
