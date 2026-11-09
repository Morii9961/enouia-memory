# ADR-MEM-131 — Linux platform layer for the Vault

2026-11-09. Morii chose a Linux VPS for the ChatGPT connector route
(ADR-MEM-129/130). Until now the Vault's only production platform was
Windows/NTFS. On other systems, free space was reported as unlimited and ACL
inspection and restriction returned `Unsupported`. That made every
owner-only root check fail, and the minimum-free-space gate meaningless.

## Decision

Add `platform/linux.rs`, selected for `target_os = "linux"`. Windows keeps its
code unchanged; other non-Windows systems keep the existing test fallback.

- **Volume.** `statfs` gives free bytes (`f_bavail * f_bsize`) and the
  filesystem magic. ext4, xfs, btrfs, zfs and f2fs count as fixed local disks.
  NFS, SMB/CIFS, FUSE and AFS count as remote and are refused by the existing
  root rule. tmpfs, overlay and anything unknown count as "other", which the
  existing non-Windows rule already accepts. Production roots belong on a
  fixed filesystem; `RootPolicy.filesystems` can require one.
- **Owner-only.** POSIX has no inheritance from the parent, so the report is
  always "protected". The owner entry is the current user when the file's uid
  equals the effective uid, System for uid 0, and Other otherwise. Any group
  bit and any other bit each count as a broad grant. A `system.posix_acl_access`
  attribute, or a default ACL on a directory, also counts as a broad grant,
  because it can name other users.
- **Restriction.** Restriction refuses a path owned by another user (no
  chown). It removes both ACL attributes, sets 0700 on directories and 0600 on
  files, then re-inspects and fails unless the result is owner-only.
- **Unchanged.** `File::try_lock` (flock), rename-and-fsync replacement,
  symlinks as reparse points, `/dev/urandom` randomness, and no cloud
  attributes.

New files inside a 0700 root are protected by the directory even when the
umask leaves them 0644. The service should still run with umask 077 (for
example `UMask=0077` in systemd).

## Not in this slice

A Linux entry that opens a real Vault and authenticates the owner. The Windows
owner bootstrap stays Windows-only. Also: the OAuth authorization server, write
tools, VPS and proxy configuration, and moving real data. The host crate does
not cross-compile on this PC (its sqlite and ring build scripts need a Linux C
compiler). The Linux unit tests were written but have not run on Linux yet.

## Consequences

The Vault's Linux owner-only and free-space gates now enforce something
instead of failing or passing everything. A `libc` dependency is added for
Linux targets only. Evidence: [validation](../validation/Linux-vault-platform.md).
