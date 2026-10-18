"""Prepare a private native staging package from enouia.portable-memory/1.

Reads only an explicitly supplied bootstrap directory. Verifies original
bytes/quotes; uses existing ChatGPT and Markdown import formats. The selected
ChatGPT-shaped copy is explicitly a derivative subset, not a complete export.
Never approves, calls a provider, or modifies the input library.
"""
import argparse
import calendar
from collections import defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path, PurePosixPath
import sqlite3
import uuid
import zipfile

def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def decode(raw):
    return json.loads(raw, object_pairs_hook=unique)


def dump(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def source_text(message):
    return "\n".join(p for p in message.get("content", {}).get("parts", []) if isinstance(p, str))


def stamp(value):
    if value is None:
        return None
    date = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if date.tzinfo is None:
        raise ValueError("timestamp lacks timezone")
    return date.astimezone(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def boundary(value, end=False):
    if value is None:
        return None
    # Portable dates are inclusive local calendar days/months, not UTC instants.
    if len(value) == 7:
        year, month = map(int, value.split("-"))
        value += f"-{calendar.monthrange(year, month)[1] if end else 1:02d}"
    if len(value) != 10:
        raise ValueError("unsupported business date")
    return stamp(value + ("T23:59:59.999+08:00" if end else "T00:00:00+08:00"))


def safe_file(root, relative):
    parts = PurePosixPath(relative.replace("\\", "/"))
    if parts.is_absolute() or ".." in parts.parts or ":" in relative:
        raise ValueError("unsafe relative source path")
    path = root.joinpath(*parts.parts)
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("source escapes library")
    for parent in [path, *path.parents]:
        if parent.is_symlink() or (hasattr(parent, "is_junction") and parent.is_junction()):
            raise ValueError("linked source refused")
        if parent == root:
            break
    return path


def md_spans(raw):
    # Notes contain one heading and one escaped JSON line. Match the native
    # Markdown adapter's bounded UTF-8/blank-line splitting exactly.
    start, end = 0, len(raw)
    out = []
    while end - start > 16 * 1024:
        limit = start + 16 * 1024
        while limit < end and raw[limit] & 0xC0 == 0x80:
            limit -= 1
        blank = raw.rfind(b"\n\n", start, limit)
        cut = blank + 2 if blank >= start else limit
        if raw[start:cut].strip():
            out.append((start, cut))
        start = cut
    if raw[start:end].strip():
        out.append((start, end))
    return out


def write_zip(path, members):
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, raw in sorted(members.items()):
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, raw)


def prepare(root, output):
    if output.exists():
        raise ValueError("output must be a new directory")
    root = root.resolve(strict=True)
    raw_seeds = (root / "memories.jsonl").read_bytes()
    seeds = [decode(line) for line in raw_seeds.decode("utf-8-sig").splitlines() if line.strip()]
    if not seeds or len(seeds) > 1000 or len({s["id"] for s in seeds}) != len(seeds):
        raise ValueError("invalid seed set")
    db = sqlite3.connect((root / "library.sqlite3").as_uri() + "?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    keys = {e["source_key"] for seed in seeds for kind in ["evidence", "supporting_context"]
            for e in seed["provenance"].get(kind, [])}
    try:
        source_rows = {key: db.execute("SELECT * FROM messages WHERE source_key=?", (key,)).fetchone() for key in keys}
    finally:
        db.close()
    originals, selected, links, attachments = {}, {}, {}, {}
    summaries = set()
    for seed in seeds:
        if seed["format"] != "enouia.portable-memory/1" or seed["status"] not in ["seed", "historical"]:
            raise ValueError("unsupported seed format/status")
        if digest(seed["content"].encode()) != seed["content_sha256"]:
            raise ValueError("seed content hash mismatch")
        if seed["review"]["owner_item_approval"] is not None or seed["review"]["native_canonical_approval"] is not False:
            raise ValueError("seed approval claims refused")
        for kind in ["evidence", "supporting_context"]:
            for evidence in seed["provenance"].get(kind, []):
                rel = evidence["archive_relative_path"]
                if rel not in originals:
                    raw = safe_file(root, rel).read_bytes()
                    originals[rel] = (digest(raw), decode(raw))
                file_hash, conversations = originals[rel]
                if file_hash != evidence["source_file_sha256"]:
                    raise ValueError("original export hash mismatch")
                pointer = evidence["locator"].split("/")[1:]
                node_id = pointer[2].replace("~1", "/").replace("~0", "~")
                conversation = conversations[int(pointer[0])]
                node = conversation["mapping"][node_id]
                message = node["message"]
                text = source_text(message)
                quote = evidence["quote"]
                if digest(quote.encode()) != evidence["quote_sha256"] or quote not in text:
                    raise ValueError("original quote mismatch: " + seed["id"])
                row = source_rows.get(evidence["source_key"])
                if row is None or row["text"] != text or row["text_sha256"] != digest(text.encode()):
                    raise ValueError("normalized source mismatch")
                content_type = message["content"].get("content_type")
                if content_type not in ["text", "multimodal_text"] or message["author"]["role"] not in ["user", "assistant"]:
                    raise ValueError("hidden or non-visible source refused")
                key = evidence["source_key"]
                selected[key] = (conversation, node_id, node)
                level = max(links.get(key, "private") == "highly_sensitive", seed["sensitivity"] == "highly_sensitive")
                links[key] = "highly_sensitive" if level else "private"
                if evidence["evidence_class"] == "imported_summary":
                    summaries.add(key)
        for kind in ["artifacts", "visual_sources", "document_sources"]:
            for artifact in seed["provenance"].get(kind, []):
                rel = artifact["archive_relative_path"]
                raw = safe_file(root, rel).read_bytes()
                if digest(raw) != artifact["source_file_sha256"]:
                    raise ValueError("artifact hash mismatch")
                if "quote" in artifact:
                    quote = artifact["quote"]
                    if digest(quote.encode()) != artifact["quote_sha256"] or quote not in raw.decode("utf-8-sig"):
                        raise ValueError("artifact quote mismatch")
                name = Path(rel).name
                if name in attachments and attachments[name] != raw:
                    raise ValueError("conflicting attachment names")
                attachments[name] = raw
    # A source shared with a sensitive record remains sensitive everywhere.
    for seed in seeds:
        if any(links[e["source_key"]] == "highly_sensitive" for k in ["evidence", "supporting_context"] for e in seed["provenance"].get(k, [])):
            seed["sensitivity"] = "highly_sensitive"
    attachment_levels = {}
    for seed in seeds:
        for kind in ["artifacts", "visual_sources", "document_sources"]:
            for artifact in seed["provenance"].get(kind, []):
                name = Path(artifact["archive_relative_path"]).name
                if name not in attachment_levels or seed["sensitivity"] == "highly_sensitive":
                    attachment_levels[name] = seed["sensitivity"]
    output.mkdir(parents=True)
    inputs, citations = [], {}
    # Put all selected sources in one conservative highly-sensitive archive.
    # Candidates retain their requested level only when all cited inputs allow it;
    # per-level archives preserve normal local recall without downgrading a source.
    for level in ["private", "highly_sensitive"]:
        groups = defaultdict(list)
        for key, (conversation, node_id, node) in selected.items():
            if links[key] == level:
                groups[conversation.get("conversation_id", conversation.get("id"))].append((key, conversation, node_id, node))
        if not groups:
            continue
        conversations = []
        for ci, (conversation_id, rows) in enumerate(sorted(groups.items())):
            original = rows[0][1]
            mapping = {node_id: node for _, _, node_id, node in rows}
            conversations.append({"id": conversation_id, "conversation_id": conversation_id,
                "mapping": mapping, "current_node": original.get("current_node"),
                "enouia_subset": {"derived": True, "complete_export": False,
                    "original_files": {rel: value[0] for rel, value in originals.items()}}})
            for key, _, node_id, node in rows:
                message = node["message"]
                role = message["author"]["role"]
                citations[key] = {"input": f"sources-{level}.zip", "locator": {
                    "kind": "archive_member", "member_name": "conversations.json", "member_hash": None,
                    "inner": {"kind": "json_pointer", "pointer": f"/{ci}/mapping/{node_id.replace('~','~0').replace('/','~1')}/message/content"}},
                    "content_hash": digest(source_text(message).encode()),
                    "evidence_class": "user_statement" if role == "user" else "model_claim", "speaker_role": role}
        raw = dump(conversations)
        for key, cite in citations.items():
            if links[key] == level:
                cite["locator"]["member_hash"] = digest(raw)
        # Preserve only explicitly reviewed supporting originals in the package.
        members = {"conversations.json": raw, **{name: data for name, data in attachments.items() if attachment_levels[name] == level}}
        name = f"sources-{level}.zip"
        write_zip(output / name, members)
        inputs.append({"name": name, "sha256": digest((output / name).read_bytes()), "sensitivity": level})
    note_citations = {}
    for level in ["private", "highly_sensitive"]:
        members = {}
        for seed in seeds:
            if seed["sensitivity"] != level:
                continue
            name = seed["id"] + ".md"
            # Full original seed (including exact quote provenance) is an
            # explicitly agent-distilled document, never new owner testimony.
            def scrub(value):
                if isinstance(value, dict):
                    return {k: scrub(v) for k, v in value.items() if k not in ["source_file", "curation_spec", "analysis_file", "audit_script"]}
                if isinstance(value, list):
                    return [scrub(v) for v in value]
                return value
            raw = ("# Agent distilled seed " + seed["id"] + "\n\n" + json.dumps(scrub(seed), ensure_ascii=False, sort_keys=True) + "\n").encode()
            members[name] = raw
            note_citations[seed["id"]] = [{"input": f"notes-{level}.zip", "locator": {
                "kind": "archive_member", "member_name": name, "member_hash": digest(raw),
                "inner": {"kind": "byte_range", "start": start, "end": end}},
                "content_hash": digest(raw[start:end]), "evidence_class": "unknown", "speaker_role": "unknown"}
                for start, end in md_spans(raw)]
        if members:
            name = f"notes-{level}.zip"
            write_zip(output / name, members)
            inputs.append({"name": name, "sha256": digest((output / name).read_bytes()), "sensitivity": level})
    proposals = []
    subject_ids = {name: "sub_" + str(uuid.uuid4()) for seed in seeds for name in seed["subjects"]}
    for seed in seeds:
        valid = seed["time"]["valid"]
        subjects = [subject_ids[s] for s in seed["subjects"]]
        details = {"title": seed["title"], "subject_ids": subjects, "category": seed["category"],
            "tags": ["seed.initialization", "context:" + seed["context"]] + (["seed.core"] if seed["priority"] == "P0" else []),
            "epistemic_status": "asserted" if seed["epistemic_status"] == "user_asserted" else "uncertain",
            "priority": seed["priority"], "volatility": seed["volatility"],
            "valid_from": boundary(valid["from"]), "valid_until": boundary(valid["until"], True),
            "observed_at": stamp(seed["time"]["observed_from"])}
        if seed["type"] == "fact":
            details["claim_key"] = seed["claim_key"]
        elif seed["type"] == "preference":
            details.update(scope=seed["claim_key"], strength="explicit")
        elif seed["type"] == "episode":
            details.update(occurred_start=boundary(valid["from"]), occurred_end=boundary(valid["until"], True),
                occurred_precision=valid["precision"] if valid["from"] else "unknown", participants=subjects, summary=seed["content"])
        elif seed["type"] == "project_state":
            project_subject = next((s for s in seed["subjects"] if s.lower() not in ["morii", "enouia"]), seed["title"])
            details.update(new_project={"display_name": project_subject, "aliases": []},
                state=[{"item_id": "itm_" + str(uuid.uuid4()),
                    "claim": seed["content"], "state_kind": seed.get("state_kind") if seed.get("state_kind") in ["planned", "decided", "implemented", "tested", "released"] else "unknown",
                    "as_of": stamp(seed["time"]["observed_until"]), "evidence_refs": []}], decisions=[], open_loops=[])
        else:
            raise ValueError("unsupported seed type")
        evidence = [citations[e["source_key"]] for e in seed["provenance"]["evidence"] if e["source_key"] not in summaries]
        context = [citations[e["source_key"]] for e in seed["provenance"].get("supporting_context", [])]
        context += [citations[e["source_key"]] for e in seed["provenance"]["evidence"] if e["source_key"] in summaries]
        # Image/document analysis and copied summaries retain the explicit
        # unknown-class derived document alongside the original statements.
        notes = note_citations[seed["id"]]
        evidence += notes if not evidence else []
        context += notes if evidence != notes else []
        unique_refs = lambda refs: list({json.dumps(r, sort_keys=True): r for r in refs}.values())
        content = seed["content"]
        if seed["context"] != "real_world_self_report":
            content = f"【语境：{seed['context']}】" + content
        proposals.append({"seed_id": seed["id"], "revision": seed["revision"], "proposed_type": seed["type"],
            "content": content, "details": details, "evidence": unique_refs(evidence), "context": unique_refs(context),
            "sensitivity": seed["sensitivity"], "reason": json.dumps({"kind": "agent_distilled_seed", "seed_id": seed["id"],
                "original_content_sha256": seed["content_sha256"], "epistemic_status": seed["epistemic_status"],
                "default_recall": seed["default_recall"], "context": seed["context"], "time": seed["time"],
                "owner_approval": False, "requires_owner_review": True}, ensure_ascii=False, sort_keys=True)})
    package = {"format": "enouia.native-seed-stage/1", "account_alias": "owner-history", "inputs": inputs, "seeds": proposals}
    raw = dump(package)
    (output / "package.json").write_bytes(raw)
    report = {"package_sha256": digest(raw), "source_library_sha256": digest(raw_seeds),
        "candidates": len(proposals), "core": sum(s["priority"] == "P0" for s in seeds),
        "selected_messages": len(selected), "reviewed_supporting_originals": len(attachments),
        "inputs": inputs, "canonical_writes": 0, "external_sends": 0}
    (output / "preparation.json").write_bytes(dump(report))
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bootstrap", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(json.dumps(prepare(args.bootstrap, args.output), ensure_ascii=False))
