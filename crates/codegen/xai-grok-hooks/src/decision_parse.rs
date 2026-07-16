//! Parse blocking-hook stdout / HTTP body into [`HookDecision`].
//!
//! Supports Grok native JSON and Claude Code / hookify shapes:
//!
//! - `{"decision":"deny","reason":"..."}` — hard block
//! - `{"decision":"block","reason":"..."}` — hard block (alias)
//! - `{"decision":"allow","reason":"..."}` — allow + soft warn when reason set
//! - Claude: `hookSpecificOutput.permissionDecision` + `additionalContext`
//! - Soft warn only: `additionalContext` / `systemMessage` without deny

use serde::Deserialize;
use serde_json::Value;

use crate::result::HookDecision;

/// Result of parsing hook stdout / HTTP JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseHookJson {
    /// Empty or whitespace-only body.
    Empty,
    /// Recognized allow/deny (including soft-warn allow).
    Decision(HookDecision),
    /// JSON with an unrecognized `decision` string and no soft context.
    UnknownDecision(String),
    /// Body present but not valid JSON.
    InvalidJson,
}

/// Claude-compatible nested object on hook stdout.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct HookSpecificOutput {
    #[serde(default)]
    permission_decision: Option<String>,
    #[serde(default)]
    additional_context: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    hook_event_name: Option<String>,
}

/// Flexible hook JSON (Grok + Claude Code / hookify).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FlexibleHookOutput {
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    system_message: Option<String>,
    #[serde(default)]
    hook_specific_output: Option<HookSpecificOutput>,
}

/// Parse blocking-hook JSON text.
pub fn parse_hook_json(body: &str, hook_name: &str) -> ParseHookJson {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return ParseHookJson::Empty;
    }

    let output = match serde_json::from_str::<FlexibleHookOutput>(trimmed) {
        Ok(o) => o,
        Err(_) => match serde_json::from_str::<Value>(trimmed) {
            Ok(v) => match serde_json::from_value::<FlexibleHookOutput>(v) {
                Ok(o) => o,
                Err(_) => return ParseHookJson::InvalidJson,
            },
            Err(_) => return ParseHookJson::InvalidJson,
        },
    };

    classify_flexible(output, hook_name)
}

/// Convenience: `Decision` only, else `None` (empty / invalid / unknown).
pub fn parse_hook_decision_json(body: &str, hook_name: &str) -> Option<HookDecision> {
    match parse_hook_json(body, hook_name) {
        ParseHookJson::Decision(d) => Some(d),
        _ => None,
    }
}

fn classify_flexible(output: FlexibleHookOutput, hook_name: &str) -> ParseHookJson {
    let hso = output.hook_specific_output.as_ref();
    let permission = hso
        .and_then(|h| h.permission_decision.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let decision = output
        .decision
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let context = first_nonempty(&[
        hso.and_then(|h| h.additional_context.as_deref()),
        output.reason.as_deref(),
        output.system_message.as_deref(),
    ]);

    let is_hard_deny = matches!(decision, Some("deny") | Some("block"))
        || matches!(permission, Some("deny") | Some("block"));

    if is_hard_deny {
        let reason = context
            .map(str::to_string)
            .unwrap_or_else(|| format!("denied by hook '{hook_name}'"));
        return ParseHookJson::Decision(HookDecision::Deny {
            reason,
            hook_name: hook_name.to_string(),
        });
    }

    // Soft context without deny => allow + warn for the model.
    if let Some(ctx) = context {
        // Known allow-ish decisions or no decision (Claude warn shape).
        if decision.is_none()
            || matches!(
                decision,
                Some("allow") | Some("continue") | Some("approve")
            )
            || permission.is_some()
        {
            return ParseHookJson::Decision(HookDecision::allow_with_context(ctx));
        }
    }

    match decision {
        None | Some("allow") | Some("continue") | Some("approve") => {
            ParseHookJson::Decision(HookDecision::allow())
        }
        Some(other) => ParseHookJson::UnknownDecision(other.to_string()),
    }
}

fn first_nonempty<'a>(candidates: &[Option<&'a str>]) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{
        format_hook_denied_for_model, format_hook_warn_for_model, prepend_hook_warn_to_tool_result,
    };

    #[test]
    fn parse_native_deny_model_text() {
        let d = match parse_hook_json(r#"{"decision":"deny","reason":"no curl"}"#, "h1") {
            ParseHookJson::Decision(d) => d,
            other => panic!("expected decision, got {other:?}"),
        };
        match d {
            HookDecision::Deny { reason, hook_name } => {
                assert_eq!(reason, "no curl");
                assert_eq!(hook_name, "h1");
                assert_eq!(
                    format_hook_denied_for_model(&reason),
                    "Hook denied: no curl"
                );
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn parse_block_alias() {
        let d = match parse_hook_json(r#"{"decision":"block","reason":"x"}"#, "h") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        assert!(d.is_deny());
    }

    #[test]
    fn parse_claude_permission_deny_surfaces_block_text() {
        let body = r#"{
          "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "additionalContext": "hookify block: use firecrawl"
          },
          "systemMessage": "blocked for human"
        }"#;
        let d = match parse_hook_json(body, "hookify") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        match d {
            HookDecision::Deny { reason, .. } => {
                assert!(
                    reason.contains("hookify block"),
                    "model must see block text, got {reason}"
                );
                assert!(format_hook_denied_for_model(&reason).starts_with("Hook denied:"));
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn parse_claude_warn_additional_context_only() {
        let body = r#"{
          "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": "hookify warn: prefer scrape tools"
          },
          "systemMessage": "warn for human"
        }"#;
        let d = match parse_hook_json(body, "hookify") {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        assert!(!d.is_deny(), "warn must not block");
        assert_eq!(
            d.additional_context(),
            Some("hookify warn: prefer scrape tools")
        );
        let tool_body = prepend_hook_warn_to_tool_result(
            d.additional_context().expect("ctx"),
            "command output ok",
        );
        assert!(
            tool_body.starts_with("Hook warn: hookify warn: prefer scrape tools"),
            "model tool_result must carry warn prefix, got {tool_body}"
        );
        assert!(tool_body.contains("command output ok"));
        assert_eq!(
            format_hook_warn_for_model("hookify warn: prefer scrape tools"),
            "Hook warn: hookify warn: prefer scrape tools"
        );
    }

    #[test]
    fn parse_allow_with_reason_is_soft_warn() {
        let d = match parse_hook_json(
            r#"{"decision":"allow","reason":"consider using rg"}"#,
            "h",
        ) {
            ParseHookJson::Decision(d) => d,
            other => panic!("{other:?}"),
        };
        assert!(!d.is_deny());
        assert_eq!(d.additional_context(), Some("consider using rg"));
    }

    #[test]
    fn parse_plain_allow() {
        assert_eq!(
            parse_hook_json(r#"{"decision":"allow"}"#, "h"),
            ParseHookJson::Decision(HookDecision::allow())
        );
    }

    #[test]
    fn empty_and_invalid() {
        assert_eq!(parse_hook_json("", "h"), ParseHookJson::Empty);
        assert_eq!(parse_hook_json("   ", "h"), ParseHookJson::Empty);
        assert_eq!(
            parse_hook_json("not-json", "h"),
            ParseHookJson::InvalidJson
        );
    }

    #[test]
    fn unknown_decision() {
        assert_eq!(
            parse_hook_json(r#"{"decision":"maybe"}"#, "h"),
            ParseHookJson::UnknownDecision("maybe".into())
        );
    }
}
