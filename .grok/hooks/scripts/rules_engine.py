#!/usr/bin/env python3
# Claude Code hookify 规则兼容

from __future__ import annotations

import glob
import json
import os
import re
import shlex
import subprocess
import sys
from dataclasses import dataclass, field, replace
from functools import lru_cache
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

_TOOL_ALIAS = {
    "run_terminal_command": "Bash",
    "run_terminal_cmd": "Bash",
    "Shell": "Bash",
    "bash": "Bash",
    "write": "Write",
    "create_file": "Write",
    "search_replace": "Edit",
    "hashline_edit": "Edit",
    "str_replace": "Edit",
    "StrReplace": "Edit",
    "read_file": "Read",
    "list_dir": "LS",
    "grep": "Grep",
    "todo_write": "TodoWrite",
}


@dataclass
class Condition:
    field: str
    operator: str
    pattern: str


@dataclass
class Rule:
    name: str
    enabled: bool
    event: str
    action: str
    message: str
    conditions: List[Condition] = field(default_factory=list)
    tool_matcher: Optional[str] = None
    file_matcher: Optional[str] = None
    command: Optional[str] = None
    hook_events: Optional[List[str]] = None
    any_of: Optional[Dict[str, List[Condition]]] = None


@lru_cache(maxsize=128)
def _re(pattern: str) -> re.Pattern:
    return re.compile(pattern, re.IGNORECASE)


def _workspace() -> Path:
    for k in ("GROK_WORKSPACE_ROOT", "CLAUDE_PROJECT_DIR", "GROK_PROJECT_DIR"):
        v = os.environ.get(k)
        if v:
            return Path(v).expanduser().resolve()
    return Path.cwd().resolve()


def _claude_rule_dirs() -> List[Path]:
    dirs = [Path.home() / ".claude", _workspace() / ".claude"]
    out: List[Path] = []
    seen = set()
    for d in dirs:
        try:
            r = d.resolve()
        except OSError:
            r = d
        key = str(r)
        if key not in seen:
            seen.add(key)
            out.append(d)
    return out


def _parse_frontmatter(text: str) -> Tuple[Dict[str, Any], str]:
    if not text.startswith("---"):
        return {}, text
    parts = text.split("---", 2)
    if len(parts) < 3:
        return {}, text
    fm: Dict[str, Any] = {}
    cur_key = None
    cur_list: list = []
    cur_dict: dict = {}
    in_list = False
    in_dict = False
    for line in parts[1].split("\n"):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        if indent == 0 and ":" in line and not s.startswith("-"):
            if in_list and cur_key:
                if in_dict and cur_dict:
                    cur_list.append(cur_dict)
                    cur_dict = {}
                fm[cur_key] = cur_list
                in_list = in_dict = False
                cur_list = []
            key, val = line.split(":", 1)
            key, val = key.strip(), val.strip()
            if not val:
                cur_key = key
                in_list = True
                cur_list = []
            else:
                if len(val) >= 2 and val[0] == val[-1] and val[0] in "\"'":
                    val = val[1:-1]
                if val.lower() == "true":
                    val = True
                elif val.lower() == "false":
                    val = False
                fm[key] = val
        elif s.startswith("-") and in_list:
            if in_dict and cur_dict:
                cur_list.append(cur_dict)
                cur_dict = {}
            item = s[1:].strip()
            if ":" in item and "," in item:
                d = {}
                for part in item.split(","):
                    if ":" in part:
                        k, v = part.split(":", 1)
                        d[k.strip()] = v.strip().strip("\"'")
                cur_list.append(d)
                in_dict = False
            elif ":" in item:
                in_dict = True
                k, v = item.split(":", 1)
                cur_dict = {k.strip(): v.strip().strip("\"'")}
            else:
                cur_list.append(item.strip("\"'"))
                in_dict = False
        elif indent > 2 and in_dict and ":" in line:
            k, v = s.split(":", 1)
            cur_dict[k.strip()] = v.strip().strip("\"'")
    if in_list and cur_key:
        if in_dict and cur_dict:
            cur_list.append(cur_dict)
        fm[cur_key] = cur_list
    return fm, parts[2].strip()


def _rule_from_fm(fm: Dict[str, Any], message: str) -> Optional[Rule]:
    if not fm:
        return None
    conds: List[Condition] = []
    raw = fm.get("conditions")
    if isinstance(raw, list):
        for c in raw:
            if isinstance(c, dict):
                conds.append(
                    Condition(
                        field=str(c.get("field", "")),
                        operator=str(c.get("operator", "regex_match")),
                        pattern=str(c.get("pattern", "")),
                    )
                )
    pattern = fm.get("pattern")
    if pattern and not conds:
        ev = str(fm.get("event", "all"))
        fld = "command" if ev == "bash" else ("new_text" if ev == "file" else "content")
        conds = [Condition(field=fld, operator="regex_match", pattern=str(pattern))]

    any_of = None
    ao = fm.get("any_of")
    if isinstance(ao, list):
        groups: Dict[str, List[Condition]] = {}
        for item in ao:
            if not isinstance(item, dict):
                continue
            g = str(item.get("group", "default"))
            groups.setdefault(g, []).append(
                Condition(
                    field=str(item.get("field", "")),
                    operator=str(item.get("operator", "regex_match")),
                    pattern=str(item.get("pattern", "")),
                )
            )
        if groups:
            any_of = groups

    he = None
    raw_he = fm.get("hook_events", fm.get("hook_event"))
    if isinstance(raw_he, str) and raw_he.strip():
        he = [raw_he.strip()]
    elif isinstance(raw_he, list):
        he = [str(x).strip() for x in raw_he if str(x).strip()] or None

    return Rule(
        name=str(fm.get("name", "unnamed")),
        enabled=bool(fm.get("enabled", True)),
        event=str(fm.get("event", "all")),
        action=str(fm.get("action", "warn")),
        message=message.strip(),
        conditions=conds,
        tool_matcher=fm.get("tool_matcher"),
        file_matcher=fm.get("file_matcher"),
        command=fm.get("command"),
        hook_events=he,
        any_of=any_of,
    )


def load_rules() -> List[Rule]:
    files: List[str] = []
    seen = set()
    for d in _claude_rule_dirs():
        for path in glob.glob(str(d / "hookify.*.local.md")):
            if not os.path.isfile(path):
                continue
            try:
                key = str(Path(path).resolve())
            except OSError:
                key = path
            if key in seen:
                continue
            seen.add(key)
            files.append(path)

    rules: List[Rule] = []
    for path in files:
        try:
            text = Path(path).read_text(encoding="utf-8")
        except OSError:
            continue
        fm, msg = _parse_frontmatter(text)
        rule = _rule_from_fm(fm, msg)
        if rule and rule.enabled:
            rules.append(rule)
    return rules


def _collect_text(obj: Any) -> str:
    parts: List[str] = []
    seen = set()

    def walk(o: Any) -> None:
        if isinstance(o, str):
            if o not in seen:
                seen.add(o)
                parts.append(o)
        elif isinstance(o, dict):
            for v in o.values():
                walk(v)
        elif isinstance(o, (list, tuple)):
            for v in o:
                walk(v)

    walk(obj)
    return " ".join(parts)


def _extract_field(
    field: str, tool_name: str, tool_input: Dict[str, Any], data: Dict[str, Any]
) -> Optional[str]:
    if field == "tool_name":
        return tool_name
    if field == "_all_text":
        return _collect_text(tool_input)
    if field in tool_input:
        v = tool_input[field]
        return v if isinstance(v, str) else str(v)
    if field == "command":
        return str(tool_input.get("command", "") or "")
    if field in ("content", "new_text", "new_string"):
        return str(
            tool_input.get("content")
            or tool_input.get("new_string")
            or tool_input.get("newString")
            or ""
        )
    if field in ("old_text", "old_string"):
        return str(tool_input.get("old_string") or tool_input.get("oldString") or "")
    if field == "file_path":
        return str(
            tool_input.get("file_path")
            or tool_input.get("target_file")
            or tool_input.get("path")
            or ""
        )
    if field == "user_prompt":
        return str(data.get("user_prompt") or data.get("userPrompt") or "")
    if field == "reason":
        return str(data.get("reason") or "")
    return None


def _check_cond(
    c: Condition, tool_name: str, tool_input: Dict[str, Any], data: Dict[str, Any]
) -> bool:
    if c.operator == "file_exists_before":
        fp = tool_input.get("file_path") or tool_input.get("target_file") or ""
        if not fp:
            return False
        exists = os.path.isfile(str(fp))
        want = c.pattern.strip().lower() not in ("false", "0", "no", "")
        return exists == want
    val = _extract_field(c.field, tool_name, tool_input, data)
    if val is None:
        return False
    op, pat = c.operator, c.pattern
    if op == "regex_match":
        try:
            return bool(_re(pat).search(val))
        except re.error:
            return False
    if op == "contains":
        return pat in val
    if op == "equals":
        return pat == val
    if op == "not_contains":
        return pat not in val
    if op == "starts_with":
        return val.startswith(pat)
    if op == "ends_with":
        return val.endswith(pat)
    return False


def _event_kind(tool_name: str) -> Optional[str]:
    if tool_name in (
        "Bash",
        "bash",
        "run_terminal_command",
        "run_terminal_cmd",
        "Shell",
    ):
        return "bash"
    if tool_name in (
        "Write",
        "Edit",
        "MultiEdit",
        "write",
        "search_replace",
        "hashline_edit",
        "StrReplace",
        "create_file",
    ):
        return "file"
    if (
        tool_name.startswith("mcp__")
        or tool_name.startswith("mcp_")
        or "__" in tool_name
    ):
        return "mcp"
    return None


def _normalize(data: Dict[str, Any], stage: str) -> Dict[str, Any]:
    name = str(data.get("toolName") or data.get("tool_name") or "")
    tin = data.get("toolInput") or data.get("tool_input") or {}
    if isinstance(tin, str) and tin.strip():
        try:
            tin = json.loads(tin)
        except json.JSONDecodeError:
            tin = {}
    if not isinstance(tin, dict):
        tin = {}

    if name in ("use_tool", "useTool"):
        nested = tin.get("tool_name") or tin.get("toolName") or ""
        nested_in = tin.get("tool_input") or tin.get("toolInput") or tin
        if isinstance(nested, str) and nested:
            name = nested
        if isinstance(nested_in, dict):
            tin = nested_in

    if "file_path" not in tin:
        for k in ("target_file", "targetFile", "path", "filePath"):
            if isinstance(tin.get(k), str):
                tin["file_path"] = tin[k]
                break
    if "content" not in tin and isinstance(tin.get("new_string"), str):
        tin["content"] = tin["new_string"]
    if "new_string" not in tin and isinstance(tin.get("newString"), str):
        tin["new_string"] = tin["newString"]
        tin.setdefault("content", tin["newString"])

    if name in _TOOL_ALIAS:
        name = _TOOL_ALIAS[name]
    elif "__" in name and not name.startswith("mcp__"):
        server, rest = name.split("__", 1)
        name = f"mcp__plugin_a_{server}__{rest}"

    raw = str(data.get("hookEventName") or data.get("hook_event_name") or stage)
    he_map = {
        "pre": "PreToolUse",
        "pre_tool_use": "PreToolUse",
        "post": "PostToolUse",
        "post_tool_use": "PostToolUse",
        "stop": "Stop",
    }
    he = he_map.get(raw.lower(), raw if raw[:1].isupper() else raw)
    return {"tool_name": name, "tool_input": tin, "hook_event_name": he, **data}


def _allowed_stage(rule: Rule, hook_event: str) -> bool:
    if rule.hook_events is not None:
        return hook_event in rule.hook_events
    if hook_event in ("PreToolUse", "PostToolUse"):
        return hook_event == ("PostToolUse" if rule.command else "PreToolUse")
    return True


def _match_tool(matcher: str, tool_name: str) -> bool:
    if matcher == "*":
        return True
    return tool_name in matcher.split("|")


def _run_command(rule: Rule, data: Dict[str, Any]) -> Optional[Rule]:
    cmd = rule.command or ""
    msg = rule.message
    cwd = str(_workspace())
    fp = (data.get("tool_input") or {}).get("file_path", "")
    if fp:
        cmd = (
            cmd.replace("{file_path}", shlex.quote(fp))
            .replace("{file_name}", shlex.quote(os.path.basename(fp)))
            .replace("{file_dir}", shlex.quote(os.path.dirname(fp)))
        )
        msg = (
            msg.replace("{file_path}", fp)
            .replace("{file_name}", os.path.basename(fp))
            .replace("{file_dir}", os.path.dirname(fp))
        )
    ok = False
    try:
        proc = subprocess.run(
            cmd, shell=True, cwd=cwd, capture_output=True, text=True, timeout=110
        )
        out = proc.stdout or ""
        ok = proc.returncode == 0
        if not ok:
            tail = f"\n[exit {proc.returncode}]"
            if proc.stderr:
                tail += f"\n{proc.stderr}"
            out = (out + tail) if out else tail.lstrip()
    except subprocess.TimeoutExpired:
        out = f"[timeout] {cmd}"
    except OSError as e:
        out = f"[error] {e}"
    if ok and not out.strip():
        return None
    if "{command_output}" in msg:
        msg = msg.replace("{command_output}", out)
    else:
        msg = msg + "\n\n命令输出:\n" + out
    return replace(rule, message=msg)


def _rule_matches(rule: Rule, data: Dict[str, Any]) -> bool:
    tool_name = data.get("tool_name") or ""
    tool_input = data.get("tool_input") or {}
    if not isinstance(tool_input, dict):
        tool_input = {}
    if rule.file_matcher:
        fp = str(tool_input.get("file_path") or "")
        try:
            if not fp or not _re(rule.file_matcher).search(fp):
                return False
        except re.error:
            return False
    if rule.tool_matcher and not _match_tool(str(rule.tool_matcher), tool_name):
        return False
    if not rule.conditions and not rule.any_of:
        return False
    if rule.any_of:
        if not any(
            all(_check_cond(c, tool_name, tool_input, data) for c in group)
            for group in rule.any_of.values()
        ):
            return False
    for c in rule.conditions:
        if not _check_cond(c, tool_name, tool_input, data):
            return False
    return True


def evaluate(rules: List[Rule], data: Dict[str, Any]) -> Dict[str, Any]:
    he = data.get("hook_event_name") or ""
    tool_name = data.get("tool_name") or ""
    kind = _event_kind(tool_name)
    blocks: List[Rule] = []
    warns: List[Rule] = []
    for rule in rules:
        if rule.event not in ("all", kind) and he != "Stop":
            if not (he == "Stop" and rule.event == "stop"):
                if rule.event != "stop" or he != "Stop":
                    if kind is None and rule.event != "all":
                        continue
                    if kind and rule.event not in ("all", kind):
                        if not (rule.event == "stop" and he == "Stop"):
                            continue
        if he == "Stop" and rule.event not in ("stop", "all"):
            continue
        if he != "Stop" and rule.event == "stop":
            continue
        if not _allowed_stage(rule, he):
            continue
        if not _rule_matches(rule, data):
            continue
        r = rule
        if r.command:
            r = _run_command(r, data)
            if r is None:
                continue
        if r.action == "block":
            blocks.append(r)
        else:
            warns.append(r)

    if blocks:
        body = "\n\n".join(f"**[{r.name}]**\n{r.message}" for r in blocks)
        return {"decision": "deny", "reason": f"[拦截] {body}"}
    if warns:
        body = "\n\n".join(f"**[{r.name}]**\n{r.message}" for r in warns)
        text = f"[警告] {body}"
        return {
            "decision": "allow",
            "reason": text,
            "systemMessage": text,
            "hookSpecificOutput": {
                "hookEventName": he or "PreToolUse",
                "additionalContext": text,
            },
        }
    return {}


def main(argv: List[str]) -> int:
    stage = (argv[1] if len(argv) > 1 else "pre").lower().strip()
    raw = sys.stdin.read()
    if not raw.strip():
        return 0
    try:
        envelope = json.loads(raw)
    except json.JSONDecodeError:
        return 0
    if not isinstance(envelope, dict):
        return 0
    data = _normalize(envelope, stage)
    result = evaluate(load_rules(), data)
    if result:
        sys.stdout.write(json.dumps(result, ensure_ascii=False) + "\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except SystemExit:
        raise
    except Exception as exc:  # noqa: BLE001
        sys.stderr.write(f"rules_engine error: {exc}\n")
        raise SystemExit(0)
