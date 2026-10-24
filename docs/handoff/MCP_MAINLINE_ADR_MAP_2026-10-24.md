# Existing MCP branch integration: ADR map

2026-10-24. Current main owns ADR-MEM-81 history comparison, 82 local
history brief and 83 supersession preview. Those IDs and files are preserved.

The already implemented MCP branch used the same three numbers independently.
Its Host-scoped Agent reads/proposals/local Context documents now use
ADR-MEM-87 / 88 / 89, with matching 087/088/089 filenames. ADR-MEM-80
restricted stdio and ADR-MEM-84 checkpoints retain their IDs.
All incoming Host-domain references are updated. This is numbering and merge
integration only; no tool, permission, activation or contract expansion.

ADR-MEM-85/86 are left available for the already existing parallel
core2-durable-jobs branch's machine-admission/jobs work. That branch is not
merged or rewritten here. When it later integrates, translate any inherited
Host 81/82/83 references through this map, retaining main's history ADRs.
Old branch reports using 55-command development surfaces remain historical;
the new combined integration manifest/log is authoritative for this commit.
Runtime continues its separate exact 08892702 / 57-command pin.
