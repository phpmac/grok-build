use std::time::Duration;

/// The outcome of a blocking (`pre_tool_use`) hook dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// Hooks allowed the tool call.
    ///
    /// `additional_context` is optional soft-warn text that must reach the
    /// model while still executing the tool (Claude Code / hookify `warn`).
    Allow {
        additional_context: Option<String>,
    },
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

/// HTTP-specific execution details for scrollback enrichment.
///
/// Populated only for `"http"` handler type hooks. Carries the target
/// URL, HTTP status, and a short preview of the response body so that
/// scrollback annotations can display them.
#[derive(Debug, Clone)]
pub struct HttpInfo {
    /// The URL that was POSTed to.
    ///
    /// **Post-expansion form**: this is the actual target the runner
    /// hit (or attempted to hit) and is intended for SSRF debugging.
    /// User `env` map values resolved at expand time can land here, so
    /// any new wire-DTO consumer that surfaces this field for **user
    /// display** MUST prefer [`raw_url`] when available -- otherwise
    /// secrets like API tokens embedded in the URL via `${TOKEN}`
    /// substitution will leak. See `HookSpec::url_raw` in
    /// `crate::config` for the parallel display-vs-execution split.
    ///
    /// [`raw_url`]: HttpInfo::raw_url
    pub url: String,
    /// Pre-expansion source URL exactly as written in the JSON file,
    /// when available. Mirrors `HookSpec::url_raw` so downstream wire
    /// DTOs / scrollback display layers can show the source string
    /// without ever leaking resolved `${VAR}` substitutions. `None`
    /// for legacy code paths that constructed the spec without the
    /// raw source (the runner falls back to displaying [`url`] in
    /// that case).
    ///
    /// [`url`]: HttpInfo::url
    pub raw_url: Option<String>,
    /// HTTP status code (e.g. 200, 500). `None` if the request never
    /// completed (timeout, connection error).
    pub status: Option<u16>,
    /// Short preview of the response body (truncated to ~200 chars).
    /// `None` if no body was read (e.g. non-blocking hooks, timeouts).
    pub response_preview: Option<String>,
}

/// The outcome of a single hook execution.
#[derive(Debug)]
pub enum HookRunResult {
    /// Hook executed successfully.
    Success {
        hook_name: String,
        elapsed: Duration,
        /// HTTP details, populated only for `"http"` handler type hooks.
        http_info: Option<HttpInfo>,
    },
    /// Hook was skipped because it is disabled.
    Skipped { hook_name: String },
    /// Hook failed (timeout, crash, bad output, etc.) — fail-open.
    Failed {
        hook_name: String,
        error: String,
        elapsed: Duration,
        /// HTTP details, populated only for `"http"` handler type hooks.
        http_info: Option<HttpInfo>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_denied_and_warn_are_model_readable_prefixes() {
        assert_eq!(
            format_hook_denied_for_model("use read_file"),
            "Hook denied: use read_file"
        );
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
    }

    #[test]
    fn allow_helpers() {
        assert_eq!(HookDecision::allow().additional_context(), None);
        assert_eq!(
            HookDecision::allow_with_context("x").additional_context(),
            Some("x")
        );
        assert!(!HookDecision::allow().is_deny());
        assert!(
            HookDecision::Deny {
                reason: "r".into(),
                hook_name: "h".into()
            }
            .is_deny()
        );
    }
}
