# Segmented Owner backup and restore

Native, offline procedure for [ADR136](../adr/136-segmented-owner-backup.md).
All paths below are placeholders. Run as the OS account owning the root. This
document does not authorize real data writes, activation, migration or deployment.

## Export and independent verification

Stop the Host/remote service, initializer and captured worker. Prepare an empty
directory on a protected local volume outside the Vault and repositories:

```text
enouia-memory-owner-backup export <STOPPED_OWNER_ROOT> <EMPTY_EXPORT>
enouia-memory-owner-backup verify <EXPORT>
```

An active Host or journal lease refuses export. An unresolved publication must
first be recovered through its existing original-operation procedure. Unknown
initialization/analysis results stay unknown in the exported journals.

The export contains private plaintext. It includes the bound descriptor, complete
reachable immutable history, all archived raw objects and the two recovery
journals. Logical deletions remain barriers on replay; this is not physical purge.
It excludes caches, orphan segments, locks, credentials and external OAuth state.
It neither selects new read policies nor extends their expiry.

Store this fixed export with the existing restic adapter on owner-selected
independent encrypted media. Keep the recovery secret separately. Restore that
encrypted snapshot into a separate empty export directory and run `verify` on it.
Only a tested copy in a separate failure domain counts as independent disaster
recovery. A local synthetic round trip is engineering evidence only.

## Restore drill and activation

Prepare another empty, verified directory on a supported local filesystem:

```text
enouia-memory-owner-backup restore <RECOVERED_EXPORT> <EMPTY_ROOT> --confirm-restore
```

Verification checks the exact file set, hashes, canonical chain, genesis binding,
admission replay and raw/attachment closure before target writes. CURRENT is
published last. A failed restore leaves its partial target isolated; preserve it
for diagnosis and choose a different empty target. There is no cleanup command.

The restore receipt blocks ordinary opening, workers, remote reads and forward
recovery. Before activation the Owner must establish that the selected export is
current: no later deletions, withdrawals or primary-authority changes are missing.
Stop the old Host so the two copies cannot both write. Then explicitly authorize:

```text
enouia-memory-owner-backup activate <RESTORED_ROOT> <CURRENT_EXPORT> --confirm-current-backup
```

Activation re-verifies the backup and restored bytes. A different head is refused;
restore the latest backup into another empty root. The flag records the Owner's
assertion; the tool cannot discover a newer offline backup or remote revocation.
It does not merge deletion ledgers, transfer primary authority or grant a policy.
After activation, inspect sources, receipts, the initializer ledger and deletion
barriers through existing native tools. Reopening an unknown operation uses its
original key, never a new analysis or resend. Starting a service is a separate step.

## Windows to Linux

The built-in [synthetic migration rehearsal](../validation/Owner-cross-platform-rehearsal.md)
exercises the same Windows export on Linux before any owner-authorized real run.

Restore the verified export on ext4/xfs/btrfs/zfs/f2fs under the service account.
Preserve the descriptor's host/Owner IDs; filesystem permissions are applied for
the destination OS account. Use that descriptor's host ID when opening the root.
Do not run bootstrap over a restored root or patch its descriptor to a new host.

Owner-data-home markers, native bootstrap registration, initialization input
archives and any other originals outside the Vault are outside this format.
Preserve and transfer them separately as explicitly selected by Morii; never
guess their paths or discard the old home. OAuth tokens/service credentials are
configured separately. VPS, domain and ChatGPT read scope remain owner decisions.

## Fresh initializer compatibility

The [fresh-run synthetic regression](../validation/Fresh-owner-backup-compatibility.md)
checks ADR145 whole-run bindings, exact journal/Raw/attachment preservation,
deletions and original-receipt-only uncertainty after local restore. It is not
real-data or independent-media recovery evidence, and does not transfer ADR110's
OS-account/canonical-home bootstrap registration to a restored location.

## Resume a fresh initialization after restore (ADR147)

After the existing explicit restore/activation steps, the trusted local native
initializer can select the activated restored root inside a protected Owner data
home without copying or replacing the old bootstrap registration:

```text
enouia-memory-fresh resume-restored <MANIFEST> <OWNER_DATA_HOME> <RESTORED_ROOT> <ACTIVATION_EXPORT> --bind-current-owner-self
enouia-memory-fresh recover-restored <MANIFEST> <OWNER_DATA_HOME> <RESTORED_ROOT> <ACTIVATION_EXPORT> --bind-current-owner-self
```

`ACTIVATION_EXPORT` is the exact verified export used for this root's activation;
it remains selected even after continued formation makes newer commits. This is
not a new assertion that an older export is current. Keep that export and the
original manifest/input objects. A subsequent restore uses its own exact activated
export. Stop the old Host before activation; no command here stops another Host,
activates a root, rebinds the descriptor or sets Runtime's selected root.

Both commands require the same existing fresh-run binding. Missing/legacy journals,
changed input, unactivated restores, another export and roots outside the chosen
home refuse. `resume-restored` never repairs publication; only `recover-restored`
requests native forward recovery after the run guard. Original unknown packets
remain receipt-only; unavailable/withheld receipts do not become permission to
retry. Pending work retains the original formation interval and requires its
policy to remain current. No bootstrap, new run, policy renewal or read grant.

See [decision](../adr/147-fresh-restored-run-resume.md) and
[synthetic verification](../validation/Fresh-restored-run-resume.md). Real use waits
for Morii's merge; local synthetic evidence is not independent-media recovery.
