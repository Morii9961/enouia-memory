"""Synthetic original/quote/time/subset bridge checks, no personal fixtures."""
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
import uuid
import zipfile

spec = importlib.util.spec_from_file_location("prepare", Path(__file__).with_name("prepare.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class Preparation(unittest.TestCase):
    def setup_library(self, root):
        text = "Synthetic: I like quiet reading."
        node = {"id": "message", "parent": "omitted-parent", "message": {
            "id": "message", "author": {"role": "user"}, "create_time": 1700000000,
            "content": {"content_type": "text", "parts": [text]}}}
        raw = prepare.dump([{"id": "conversation", "current_node": "other-branch", "mapping": {"message": node}}])
        (root / "raw").mkdir()
        (root / "raw/export.json").write_bytes(raw)
        quote = {"archive_relative_path": "raw/export.json", "source_file_sha256": prepare.digest(raw),
            "locator": "/0/mapping/message/message", "source_key": "conversation/message",
            "quote": text, "quote_sha256": prepare.digest(text.encode()), "evidence_class": "user_statement"}
        seed = {"format": "enouia.portable-memory/1", "id": "synthetic", "revision": 1,
            "type": "episode", "status": "historical", "claim_key": "synthetic.reading", "title": "Synthetic reading",
            "content": text, "content_sha256": prepare.digest(text.encode()), "subjects": ["Synthetic"],
            "priority": "P0", "category": "reading", "sensitivity": "private", "volatility": "stable",
            "context": "fictional_character", "default_recall": True, "epistemic_status": "user_asserted",
            "time": {"valid": {"from": "2026-01-18", "until": "2026-01-18", "precision": "day"},
                "observed_from": "2026-01-18T00:00:00Z", "observed_until": "2026-01-18T00:00:00Z"},
            "provenance": {"evidence": [quote], "supporting_context": [], "artifacts": [], "visual_sources": [], "document_sources": []},
            "review": {"owner_item_approval": None, "native_canonical_approval": False}}
        with sqlite3.connect(root / "library.sqlite3") as db:
            db.execute("CREATE TABLE messages (source_key TEXT,text TEXT,text_sha256 TEXT)")
            db.execute("INSERT INTO messages VALUES (?,?,?)", (quote["source_key"], text, prepare.digest(text.encode())))
        db.close()
        self.write_seed(root, seed)
        return seed

    def write_seed(self, root, seed):
        (root / "memories.jsonl").write_text(json.dumps(seed) + "\n", encoding="utf-8")

    def test_preserves_dates_parent_fiction_and_uses_native_v4_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.setup_library(root)
            a = prepare.prepare(root, root / "a")
            b = prepare.prepare(root, root / "b")
            self.assertEqual(a["inputs"], b["inputs"])
            package = json.loads((root / "a/package.json").read_bytes())
            seed = package["seeds"][0]
            self.assertEqual(uuid.UUID(seed["details"]["subject_ids"][0][4:]).version, 4)
            self.assertIn("fictional_character", seed["content"])
            self.assertIsNone(seed["details"]["valid_from"])
            self.assertIsNone(seed["details"]["valid_until"])
            self.assertEqual(seed["details"]["occurred_start"], "2026-01-17T16:00:00.000Z")
            self.assertEqual(seed["details"]["occurred_end"], "2026-01-18T15:59:59.999Z")
            self.assertTrue(seed["context"])
            with zipfile.ZipFile(root / "a/sources-private.zip") as archive:
                source = json.loads(archive.read("conversations.json"))[0]
            self.assertFalse(source["enouia_subset"]["complete_export"])
            self.assertEqual(source["mapping"]["message"]["parent"], "omitted-parent")
            self.assertEqual(source["current_node"], "other-branch")

    def test_changed_original_and_fake_approval_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            seed = self.setup_library(root)
            seed["review"]["native_canonical_approval"] = True
            self.write_seed(root, seed)
            with self.assertRaisesRegex(ValueError, "approval"):
                prepare.prepare(root, root / "a")
            seed["review"]["native_canonical_approval"] = False
            self.write_seed(root, seed)
            (root / "raw/export.json").write_text("[]")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                prepare.prepare(root, root / "a")

    def test_copied_summary_remains_context_instead_of_native_user_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            seed = self.setup_library(root)
            seed["provenance"]["evidence"][0]["evidence_class"] = "imported_summary"
            self.write_seed(root, seed)
            prepare.prepare(root, root / "a")
            row = json.loads((root / "a/package.json").read_bytes())["seeds"][0]
            self.assertTrue(all(c["evidence_class"] == "unknown" for c in row["evidence"]))
            self.assertTrue(any(c["evidence_class"] == "user_statement" for c in row["context"]))

    def test_month_boundaries_and_utf8_spans(self):
        self.assertEqual(prepare.boundary("2024-02", True), "2024-02-29T15:59:59.999Z")
        raw = ("# Note\n\n" + "字" * 20000).encode()
        spans = prepare.md_spans(raw)
        self.assertEqual(b"".join(raw[a:b] for a, b in spans), raw)
        for a, b in spans:
            raw[a:b].decode()
            self.assertLessEqual(b - a, 16 * 1024)


if __name__ == "__main__":
    unittest.main()
