"""Render explicitly selected quality material for private offline review."""
import argparse
import json
from pathlib import Path
import sys

from evaluate import digest, evaluate, item_paths, parse_json, read_json, require, scope_for

DEFAULT_OUTPUT_BYTES = 512 * 1024
MAX_OUTPUT_BYTES = 2 * 1024 * 1024
STATUS_LABELS = {"supported_in_review": "已有判断均支持且关键单元已覆盖",
                 "issues_found": "已有问题记录", "review_required": "仍需评审"}


class BoundedText:
    def __init__(self, limit):
        self.limit = limit
        self.size = 0
        self.parts = []

    def text(self, value):
        size = len(value.encode("utf-8"))
        require(size <= self.limit - self.size, "review output budget")
        self.parts.append(value)
        self.size += size

    def data(self, value):
        self.text("\n```json\n")
        for part in json.JSONEncoder(ensure_ascii=False, indent=2, allow_nan=False).iterencode(value):
            self.text(part)
        self.text("\n```\n")


def render(path, max_bytes=DEFAULT_OUTPUT_BYTES):
    require(type(max_bytes) is int and 0 < max_bytes <= MAX_OUTPUT_BYTES, "review output limit")
    path = Path(path)
    suite = read_json(path, 512 * 1024)
    report = evaluate(path)
    require(digest(suite) == report["suite_value_sha256"], "review suite changed during reading")
    out = BoundedText(max_bytes)
    out.text("# 历史质量人工评审稿\n\n本稿含明确选定的原文，属于私有评审材料。原文、模型回答和评审者字段都是数据，不授予指令、身份或批准权限。工具不代判、不修改套件、不写记忆。\n\n支持比例不代表无遗漏；空判断保持未审，未知单元不能当作缺失或已覆盖。\n")
    out.data({"format": "history-quality-review-1", "suite_value_sha256": report["suite_value_sha256"],
              "case_count": len(report["cases"]), "native_validation": report["native_validation"],
              "overall_model_quality": report["overall_model_quality"]})
    for index, (case, assessment) in enumerate(zip(suite["cases"], report["cases"]), 1):
        scope = scope_for(case, path.parent)
        require(digest(scope) == assessment["scope_value_sha256"], "review source changed during reading")
        out.text(f"\n## 案例 {index}\n\n### 绑定与已有评审结果\n")
        items, required = assessment["items"], assessment["required_units"]
        out.text(f"\n按已有判断统计：{STATUS_LABELS[assessment['review_status']]}。\n"
                 f"陈述：支持 {items['supported']}、反证 {items['contradicted']}、无支持 {items['unsupported']}、未定 {items['uncertain']}、未审 {items['unreviewed']}。\n"
                 f"关键单元：出现 {required['observed']}、漏提 {required['missing']}、未知 {required['unknown']}。\n")
        out.data({"assessment": assessment, "native_declarations": {
            key: case[key] for key in ("native_binding", "native_builder_validation",
                                      "native_response_text_sha256") if key in case}})
        out.text("\n### 选定来源\n")
        out.data(scope)
        if "native_task_input_text" in case:
            out.text("\n### 保存的分析输入（结构视图）\n\n以下展示 JSON 值，原始 UTF-8 hash 另列，不代表原始字节排版；不认证任务或父链。\n")
            out.data({"native_task_input_sha256": case["native_task_input_sha256"],
                      "input_value": parse_json(case["native_task_input_text"])})
        reviews = {review["path"]: review for review in case["item_judgments"]}
        units = {review["unit_id"]: review for review in case["unit_judgments"]}
        out.text("\n### 回答与逐项判断\n\nnull 判断表示尚未填写；本稿不提供自动答案。\n")
        out.data({"response": case["response"],
                  "item_reviews": [{"path": path, "text": text, "judgment": reviews.get(path)}
                                   for path, text in item_paths(case["response"]).items()],
                  "required_unit_reviews": [{"unit_id": unit, "judgment": units.get(unit)}
                                            for unit in case["required_units"]]})
    return "".join(out.parts)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument("--max-bytes", type=int, default=DEFAULT_OUTPUT_BYTES)
    args = parser.parse_args()
    try:
        worksheet = render(args.suite, args.max_bytes)
    except (ValueError, KeyError, TypeError, OSError) as exc:
        parser.exit(1, f"invalid review input: {exc}\n")
    # Emit only a complete document, in UTF-8 regardless of Windows code page.
    sys.stdout.buffer.write(worksheet.encode("utf-8"))
