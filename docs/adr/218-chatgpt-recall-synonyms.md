# ADR-MEM-218 — ChatGPT adds synonyms to recall queries

Date: 2026-11-26. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

Recall matches words, not meanings (ADR-MEM-209 to 216). A memory worded
"首次饮酒" answers "我第一次喝酒是什么时候" only through the shared
character 酒.

Morii plans semantic recall with a local embedding model on his own computer
(ADR to follow). The remote service, on a small VPS, cannot run that model.
So when ChatGPT asks, nothing on the server can turn the question into a
vector. ChatGPT is itself a language model, though, and can supply the
missing words.

## Decision

The ChatGPT short form of `memory_recall` changes only its descriptions:

- The tool asks ChatGPT to add a few synonyms and related words after the
  question, for example "首次 饮酒 啤酒" for 我第一次喝酒是什么时候.
- The `text` field says the same thing.

The suggested custom instructions in CHATGPT_CONNECTOR_USAGE.md repeat it.

The server does nothing new. The added words are ordinary query words, and
recall weighs them as usual.

## Verification

- **Contract.** In the `attention` drink test, the question followed by
  "首次 饮酒 啤酒" still ranks the 首次饮酒 memory first.
- **Remote suites.** They pass unchanged, and each tool keeps its own
  description.
