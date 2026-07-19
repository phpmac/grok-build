//! 本地 fork 回归: soft-warn / hookify 协议 + 规则引擎产出的 JSON 形状.
//!
//! 目标: 与官方 monorepo 合并后, 这些断言失败 = 本地独有能力被冲坏.
//! 配套: `.grok/hooks/scripts/rules_engine.py --self-test` 与
//! `examples/hooks/bin/chinese-punctuation-warn.py --self-test`.

#[cfg(test)]
mod tests {
    use crate::decision_parse::{ParseHookJson, parse_hook_json};
    use crate::result::{
        HookDecision, format_hook_denied_for_model, format_hook_warn_for_model,
        prepend_hook_warn_to_tool_result,
    };
    use std::path::PathBuf;
    use std::process::Command;

    /// 仓库根: crates/codegen/xai-grok-hooks -> ../../..
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root")
    }

    fn run_py_self_test(rel: &str) {
        let script = repo_root().join(rel);
        assert!(script.is_file(), "missing {rel} at {}", script.display());
        let out = Command::new("python3")
            .arg(&script)
            .arg("--self-test")
            .output()
            .unwrap_or_else(|e| panic!("spawn python3 {rel}: {e}"));
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{rel} --self-test failed\nstatus={}\nstdout={stdout}\nstderr={stderr}",
            out.status
        );
    }

    #[test]
    fn rules_engine_self_test_passes() {
        // 中文标点 warn / github 禁爬虫与 curl / gh 放行 (fixture 规则, 不依赖 ~/.claude)
        run_py_self_test(".grok/hooks/scripts/rules_engine.py");
    }

    #[test]
    fn chinese_punctuation_script_self_test_passes() {
        run_py_self_test(
            "crates/codegen/xai-grok-hooks/examples/hooks/bin/chinese-punctuation-warn.py",
        );
    }

    /// rules_engine 中文标点 hit 时 stdout 形状: allow + reason + additionalContext.
    #[test]
    fn rules_engine_chinese_punct_json_is_soft_warn_not_block() {
        let body = r#"{
          "decision": "allow",
          "reason": "[警告] **[warn-chinese-punctuation]**\n**避免使用中文标点符号或 emoji 表情**",
          "systemMessage": "[警告] **[warn-chinese-punctuation]**",
          "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": "[警告] **[warn-chinese-punctuation]**\n**避免使用中文标点符号或 emoji 表情**"
          }
        }"#;
        let d = match parse_hook_json(body, "project-rules") {
            ParseHookJson::Decision(d) => d,
            other => panic!("expected decision, got {other:?}"),
        };
        assert!(!d.is_deny(), "标点规则必须 soft-warn 放行, 不得 deny");
        let ctx = d.additional_context().expect("soft-warn context");
        assert!(
            ctx.contains("中文标点") || ctx.contains("warn-chinese-punctuation"),
            "model must see punct warn, got {ctx}"
        );
        let merged = prepend_hook_warn_to_tool_result(ctx, "tool ok");
        assert!(
            merged.starts_with("Hook warn:"),
            "tool_result prefix, got {merged}"
        );
        assert!(merged.contains("tool ok"));
    }

    /// rules_engine block-github / block-curl 时 stdout 形状: deny + reason.
    #[test]
    fn rules_engine_github_block_json_is_deny_with_reason() {
        let body = r#"{
          "decision": "deny",
          "reason": "[拦截] **[block-github-via-scraper]**\n访问github必须使用gh命令"
        }"#;
        let d = match parse_hook_json(body, "project-rules") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        match d {
            HookDecision::Deny { reason, .. } => {
                assert!(
                    reason.contains("gh") || reason.contains("拦截"),
                    "block reason for model, got {reason}"
                );
                assert!(format_hook_denied_for_model(&reason).starts_with("Hook denied:"));
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn claude_hookify_block_github_curl_shape() {
        let body = r#"{
          "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "additionalContext": "禁止使用 curl 访问github,按照规范使用gh"
          }
        }"#;
        let d = match parse_hook_json(body, "hookify") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        assert!(d.is_deny());
        let reason = match d {
            HookDecision::Deny { reason, .. } => reason,
            _ => unreachable!(),
        };
        assert!(reason.contains("gh") || reason.contains("github"));
    }

    #[test]
    fn post_tool_use_allow_with_reason_not_dropped_by_blocking_parser() {
        // 非阻塞路径与阻塞路径共用 decision_parse; allow+reason 必须是 soft-warn
        let body = r#"{"decision":"allow","reason":"slither: medium finding on foo.sol"}"#;
        let d = match parse_hook_json(body, "rules_engine") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        assert!(!d.is_deny());
        assert_eq!(
            d.additional_context(),
            Some("slither: medium finding on foo.sol")
        );
        assert_eq!(
            format_hook_warn_for_model("slither: medium finding on foo.sol"),
            "Hook warn: slither: medium finding on foo.sol"
        );
    }

    /// 端到端: 真实跑 chinese-punctuation 脚本 stdin, 再 parse 成 soft-warn.
    #[test]
    fn chinese_punctuation_script_stdout_parses_as_soft_warn() {
        let script = repo_root().join(
            "crates/codegen/xai-grok-hooks/examples/hooks/bin/chinese-punctuation-warn.py",
        );
        let envelope = r#"{
          "hookEventName": "PreToolUse",
          "toolName": "write",
          "toolInput": {"file_path": "/tmp/t.md", "content": "你好，世界。"}
        }"#;
        let out = Command::new("python3")
            .arg(&script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child
                    .stdin
                    .as_mut()
                    .expect("stdin")
                    .write_all(envelope.as_bytes())?;
                child.wait_with_output()
            })
            .expect("run chinese-punctuation-warn");
        assert!(out.status.success(), "script fail: {:?}", out);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "expect soft-warn JSON on chinese punct"
        );
        let d = match parse_hook_json(stdout.trim(), "chinese-punctuation-warn") {
            ParseHookJson::Decision(d) => d,
            other => panic!("parse {other:?} from {stdout}"),
        };
        assert!(!d.is_deny());
        assert!(d.additional_context().is_some());
    }

    /// 端到端: 仅加载 fixture 规则目录, firecrawl+github 必须 deny.
    #[test]
    fn rules_engine_fixture_dir_blocks_github_via_scraper() {
        let script = repo_root().join(".grok/hooks/scripts/rules_engine.py");
        let fixtures = repo_root().join(".grok/hooks/fixtures/rules");
        // 临时把 CLAUDE 规则目录指到 fixture: 用空 HOME + 工作区 .claude 软链太重.
        // 改为内联 python 调 load_rules_from_dirs + evaluate (与 --self-test 同源).
        let script_dir = script.parent().expect("dir").display().to_string();
        let fixtures_s = fixtures.display().to_string();
        let py = format!(
            r#"
import sys
sys.path.insert(0, {script_dir:?})
from rules_engine import load_rules_from_dirs, evaluate, _normalize
from pathlib import Path
rules = load_rules_from_dirs([Path({fixtures_s:?})])
env = {{
  "toolName": "firecrawl__scrape",
  "toolInput": {{"url": "https://github.com/xai-org/grok-build"}},
}}
data = _normalize(env, "pre")
result = evaluate(rules, data)
assert result.get("decision") == "deny", result
assert "gh" in result.get("reason", "") or "拦截" in result.get("reason", ""), result
print("ok")
"#
        );
        let out = Command::new("python3")
            .arg("-c")
            .arg(py)
            .output()
            .expect("python fixture evaluate");
        assert!(
            out.status.success(),
            "fixture evaluate failed: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// gh 访问 github 不被 scraper/curl 规则误伤.
    #[test]
    fn rules_engine_fixture_allows_gh_cli() {
        let script = repo_root().join(".grok/hooks/scripts/rules_engine.py");
        let fixtures = repo_root().join(".grok/hooks/fixtures/rules");
        let script_dir = script.parent().expect("dir").display().to_string();
        let fixtures_s = fixtures.display().to_string();
        let py = format!(
            r#"
import sys
sys.path.insert(0, {script_dir:?})
from rules_engine import load_rules_from_dirs, evaluate, _normalize
from pathlib import Path
rules = load_rules_from_dirs([Path({fixtures_s:?})])
env = {{
  "toolName": "run_terminal_command",
  "toolInput": {{"command": "gh api repos/xai-org/grok-build"}},
}}
data = _normalize(env, "pre")
result = evaluate(rules, data)
assert result == {{}}, result
print("ok")
"#
        );
        let out = Command::new("python3")
            .arg("-c")
            .arg(py)
            .output()
            .expect("python gh allow");
        assert!(
            out.status.success(),
            "gh allow failed: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
