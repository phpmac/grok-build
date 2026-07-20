use std::time::Duration;

/// The outcome of a blocking (`pre_tool_use`) hook dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// Hooks allowed the tool call.
    ///
    /// `additional_context` is optional soft-warn text that must reach the
    /// model while still executing the tool (Claude Code / hookify `warn`).
    Allow { additional_context: Option<String> },
    /// At least one hook denied with the given reason (tool must not run).
    Deny { reason: String, hook_name: String },
}

impl HookDecision {
    /// Allow with no extra context for the model.
    pub const fn allow() -> Self {
        Self::Allow {
            additional_context: None,
        }
    }

    /// Allow, and surface `ctx` to the model (hook warn / soft inject).
    pub fn allow_with_context(ctx: impl Into<String>) -> Self {
        let s = ctx.into();
        if s.is_empty() {
            Self::allow()
        } else {
            Self::Allow {
                additional_context: Some(s),
            }
        }
    }

    /// Soft-warn / additional context if this is an Allow with text.
    pub fn additional_context(&self) -> Option<&str> {
        match self {
            Self::Allow {
                additional_context: Some(s),
            } => Some(s.as_str()),
            _ => None,
        }
    }

    /// True when the tool call must be blocked.
    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }
}

/// Model-visible prefix for a blocked tool (tool_result content).
pub fn format_hook_denied_for_model(reason: &str) -> String {
    format!("Hook denied: {reason}")
}

/// Model-visible prefix for a soft warn (prepended to successful tool_result).
pub fn format_hook_warn_for_model(message: &str) -> String {
    format!("Hook warn: {message}")
}

/// Merge soft-warn text onto a successful tool result body.
pub fn prepend_hook_warn_to_tool_result(warn: &str, tool_result: &str) -> String {
    if tool_result.is_empty() {
        format_hook_warn_for_model(warn)
    } else {
        format!("{}\n\n{tool_result}", format_hook_warn_for_model(warn))
    }
}

/// Parsed output of one `Stop`/`SubagentStop` gate hook. The dispatcher
/// aggregates these across hooks; `force_stop` overrides blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopHookOutcome {
    pub block_reason: Option<String>,
    pub additional_context: Option<String>,
    pub force_stop: Option<StopOverride>,
}

/// A `continue: false` force-stop; `reason` is `stopReason`, shown to the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopOverride {
    pub reason: Option<String>,
}

impl StopHookOutcome {
    pub fn is_empty(&self) -> bool {
        self.block_reason.is_none()
            && self.additional_context.is_none()
            && self.force_stop.is_none()
    }
}

/// HTTP execution details for `"http"` hooks, for scrollback enrichment.
#[derive(Debug, Clone)]
pub struct HttpInfo {
    /// Post-expansion target (for SSRF debugging). May contain secrets from
    /// resolved `${VAR}` substitutions, so user-facing display MUST prefer
    /// `raw_url` when present.
    pub url: String,
    /// Pre-expansion source URL as written in the file, safe for display.
    /// `None` when the spec was built without it (fall back to `url`).
    pub raw_url: Option<String>,
    pub status: Option<u16>,
    pub response_preview: Option<String>,
}

/// The outcome of a single hook execution.
#[derive(Debug)]
pub enum HookRunResult {
    Success {
        hook_name: String,
        elapsed: Duration,
        http_info: Option<HttpInfo>,
    },
    Skipped {
        hook_name: String,
    },
    /// Ran and blocked: a stop-gate decision, not a failure (distinct from `Failed`).
    Blocked {
        hook_name: String,
        detail: String,
        elapsed: Duration,
        http_info: Option<HttpInfo>,
    },
    /// Hook failed (timeout, crash, bad output): fail-open.
    Failed {
        hook_name: String,
        error: String,
        elapsed: Duration,
        http_info: Option<HttpInfo>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_warn_helpers_roundtrip() {
        assert_eq!(
            format_hook_warn_for_model("prefer firecrawl"),
            "Hook warn: prefer firecrawl"
        );
        assert_eq!(
            prepend_hook_warn_to_tool_result("tip", "ok body"),
            "Hook warn: tip\n\nok body"
        );
        assert_eq!(
            prepend_hook_warn_to_tool_result("tip", ""),
            "Hook warn: tip"
        );
        assert_eq!(format_hook_denied_for_model("no"), "Hook denied: no");
    }

    #[test]
    fn allow_with_context_empty_is_plain_allow() {
        assert_eq!(HookDecision::allow().additional_context(), None);
        assert_eq!(
            HookDecision::allow_with_context("x").additional_context(),
            Some("x")
        );
        assert_eq!(
            HookDecision::allow_with_context("").additional_context(),
            None
        );
        assert!(!HookDecision::allow().is_deny());
        assert!(HookDecision::Deny {
            reason: "r".into(),
            hook_name: "h".into(),
        }
        .is_deny());
    }
}
