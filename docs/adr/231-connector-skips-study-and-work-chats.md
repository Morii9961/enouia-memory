# ADR-MEM-231 — The connector leaves study and work chats out

Date: 2026-11-27. Status: Accepted (Morii's decision of 2026-11-27).

## Context

In Runtime's real-library tests Morii found study and work material among his
memories. He kept a memory when any of its evidence came from his everyday
Enouia chats, and had the ones drawn only from separate study or work chats
forgotten through the existing Owner Forget action (8 of 351). He then asked
that daily capture not take in such chats again.

ChatGPT decides what to submit (ADR-MEM-157); Memory checks quotes, kinds and
limits but cannot tell what a chat is for.

## Decision

The `memory_submit_conversation` description tells ChatGPT to submit only
Morii's everyday chats with Enouia, never a chat he opened for study
(explaining problems, homework, course material) or for work (coding, tool
setup, project tasks), and not even that chat's turns. What Morii says about
his own studies or work inside an everyday chat, such as an exam, a result or
how he felt, still counts. The connector usage guide and its suggested custom
instructions say the same.

No server-side classifier is added: the rule follows the chat's purpose,
which only the distilling model sees. A study or work memory that slips
through is removed the same way, with Owner Forget.

## Consequences

Only the tool text and the guide change; inputs, validation and stored
formats stay as they are. A connector needs the new binary (or a restart on
it) for ChatGPT to see the new description.
