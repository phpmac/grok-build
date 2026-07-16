#!/usr/bin/env python3
"""Grok PreToolUse: Write/Edit 内容含中文标点或 emoji 时 soft-warn.

协议 (与本仓库 xai-grok-hooks 一致):
  - 静默放行: exit 0, 无 stdout
  - soft-warn 放行: exit 0, stdout JSON
      {"decision":"allow","reason":"..."}
    由 decision_parse 解析为 Allow { additional_context: Some(reason) },
    工具照常执行, 模型在 tool_result 前看到 Hook warn: ...
  - 不使用 deny (本规则是文案规范提醒, 不是硬拦截)

自测: python3 chinese-punctuation-warn.py --self-test
"""

from __future__ import annotations

import json
import re
import sys
from typing import Any

# 中文标点 + 常见 emoji/符号区段 (与仓库文案规范一致: 中文可用, 标点用英文)
CHINESE_PUNCT = re.compile(
    r"[，。！？；：“”‘’【】《》、"
    r"\U0001F000-\U0001FAFF"
    r"\U00002300-\U000023FF"
    r"\U00002600-\U000027BF"
    r"\U00002B00-\U00002BFF"
    r"\U0000FE00-\U0000FE0F]"
)

# 写入类工具 input 里可能承载正文的字段
CONTENT_KEYS = frozenset(
    {
        "content",
        "new_string",
        "newString",
        "old_string",
        "oldString",
        "contents",
        "text",
        "file_text",
        "fileText",
    }
)

WARN_REASON = (
    "检测到中文标点或 emoji. "
    "本仓库文案: 简体中文 + 英文标点 (禁 ，。！？；：等全角标点). "
    "请改成英文标点后重写/再编辑."
)

# 工具名白名单 (Grok + 外部别名); matcher 已过滤, 脚本内再兜底
FILE_TOOLS = frozenset(
    {
        "write",
        "Write",
        "search_replace",
        "hashline_edit",
        "Edit",
        "MultiEdit",
        "NotebookEdit",
    }
)


def extract_tool_name(envelope: dict[str, Any]) -> str:
    return str(
        envelope.get("toolName")
        or envelope.get("tool_name")
        or envelope.get("payload", {}).get("toolName")
        or ""
    )


def extract_tool_input(envelope: dict[str, Any]) -> dict[str, Any]:
    raw = (
        envelope.get("toolInput")
        or envelope.get("tool_input")
        or envelope.get("payload", {}).get("toolInput")
        or {}
    )
    return raw if isinstance(raw, dict) else {}


def collect_text_blobs(tool_input: dict[str, Any]) -> list[str]:
    """从 tool_input 抽出待检文本 (优先已知字段, 否则扫所有 str 值)."""
    blobs: list[str] = []
    for key, val in tool_input.items():
        if key in CONTENT_KEYS and isinstance(val, str) and val:
            blobs.append(val)
    if blobs:
        return blobs
    # 兜底: 任意足够长的字符串字段 (避免把 path 误报, path 通常无中文标点)
    for key, val in tool_input.items():
        if key in ("file_path", "filePath", "path", "target_file", "targetFile"):
            continue
        if isinstance(val, str) and len(val) >= 1:
            blobs.append(val)
    return blobs


def has_chinese_punctuation(text: str) -> bool:
    return CHINESE_PUNCT.search(text) is not None


def should_warn(envelope: dict[str, Any]) -> bool:
    name = extract_tool_name(envelope)
    if name and name not in FILE_TOOLS:
        # matcher 已限文件工具; 若 stdin 无 toolName 则仍检 content
        pass
    tool_input = extract_tool_input(envelope)
    if not tool_input:
        return False
    return any(has_chinese_punctuation(t) for t in collect_text_blobs(tool_input))


def emit_soft_warn() -> None:
    # Grok native soft-warn: allow + reason -> additional_context for model
    print(
        json.dumps(
            {
                "decision": "allow",
                "reason": WARN_REASON,
                "systemMessage": WARN_REASON,
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "additionalContext": WARN_REASON,
                },
            },
            ensure_ascii=False,
        )
    )


SELF_TEST_CASES: list[tuple[str, bool]] = [
    # (sample text, should match)
    ("hello world", False),
    ("注释: 使用英文标点", False),  # 中文汉字 + 英文冒号, 不匹配
    ("你好，世界", True),
    ("结束。", True),
    ("注意！", True),
    ("吗？", True),
    ("项目；模块", True),
    ("标题：说明", True),
    ("“引号”", True),
    ("【括号】", True),
    ("《书名》", True),
    ("顿号、分隔", True),
    ("ok 😀", True),
]


def self_test() -> int:
    failures = 0
    for text, want in SELF_TEST_CASES:
        got = has_chinese_punctuation(text)
        if got != want:
            failures += 1
            print(f"FAIL pattern: {text!r} -> {got} (want {want})")

    # envelope 级: Write content 含中文标点
    env_hit = {
        "hookEventName": "PreToolUse",
        "toolName": "write",
        "toolInput": {
            "file_path": "/tmp/t.md",
            "content": "你好，世界。",
        },
    }
    env_miss = {
        "hookEventName": "PreToolUse",
        "toolName": "write",
        "toolInput": {
            "file_path": "/tmp/t.md",
            "content": "hello, world.",
        },
    }
    env_edit = {
        "toolName": "search_replace",
        "toolInput": {
            "file_path": "a.rs",
            "old_string": "x",
            "new_string": "注释：错误标点",
        },
    }
    for label, env, want in [
        ("write_hit", env_hit, True),
        ("write_miss", env_miss, False),
        ("edit_hit", env_edit, True),
    ]:
        got = should_warn(env)
        if got != want:
            failures += 1
            print(f"FAIL envelope {label}: {got} (want {want})")

    total = len(SELF_TEST_CASES) + 3
    print(f"{total - failures}/{total} passed")
    return 1 if failures else 0


def main() -> None:
    if "--self-test" in sys.argv[1:]:
        sys.exit(self_test())
    try:
        envelope = json.load(sys.stdin)
    except (ValueError, OSError):
        sys.exit(0)  # fail-open
    if not isinstance(envelope, dict):
        sys.exit(0)
    if should_warn(envelope):
        emit_soft_warn()
    sys.exit(0)


if __name__ == "__main__":
    main()
