# ADR-MEM-154 — Relocate a registered Morii data home on the same machine

Date: 2026-11-19. Status: Accepted (Morii's decision); synthetic validation.

## Context

The native bootstrap registration (ADR-MEM-110/145) records the home's canonical
absolute path, the Windows account, the operation, the Vault descriptor and its
hash. `registered_owner_root` refuses any home whose path differs from the
recorded one, so moving the home breaks Runtime's registered read-only entry.
Morii wants all data in one data home folder and asked for the current home to
be moved there. Inside the home, the Vault root is recorded relative to the home
(`vaults/<vault-id>`) and no other Vault or home file embeds the absolute path.

## Decision

Add the trusted native command
`enouia-memory-owner-bootstrap relocate <new-home> <operation> <recorded-old-home> --bind-current-owner-self`.
Morii first moves the whole home directory on the same machine, then runs the
command at the new location. It succeeds only when all of these hold:

- the new home passes the usual home checks: protected marker, current-user-only
  ACL, not under the temporary directory, and the same OS account as registered;
- the registration names exactly the given operation, is in the `recorded` phase,
  and its recorded home equals `<recorded-old-home>` character for character;
- nothing exists at the old path any more, so a copied home cannot leave two live
  registrations for one Vault;
- the Vault descriptor under the new home equals the registered descriptor.

It rewrites only `canonical_home`, through the existing atomic registration write
under the home lock. The registration format, version, operation, descriptor and
all other fields stay the same, so readers already pinned to an earlier revision
can open the relocated home. Running it again at the new location returns the
same result without writing. A wrong operation, a different recorded path, an
existing old path, another account or a changed descriptor is refused.

## Boundaries

No new registration field, Vault commit, grant or descriptor change. The command
does not move files; it never deletes the old location or anything else. It is
not for copying a home to a second machine or for restored roots, which keep
ADR-MEM-136/147. Runtime only needs the new `--memory-owner-home` path; no
pin change is required for relocation itself.

## Verification

Synthetic homes with a test identity:

- after a rename, the registered root is refused until relocation and opens
  afterwards;
- a second relocation is a no-op;
- an existing directory at the old path, a mismatched recorded path, another
  operation and another account are each refused, leaving the registration bytes
  unchanged.
