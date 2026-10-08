use std::path::Path;

use xai_grok_hooks::discovery::DiscoveryOptions;
use xai_grok_hooks::error::HookError;
use xai_grok_hooks::trust::DisabledHooks;
use xai_grok_hooks::trust::Trust;
use xai_grok_workspace::hook_inputs::ProcessHookInputs;
use xai_grok_workspace::permission::resolution::managed_settings;

pub(crate) fn process_hook_inputs() -> ProcessHookInputs {
    ProcessHookInputs::read(crate::claude_import::import_marker())
}

/// For a session that dispatches the hooks it discovers.
pub(crate) fn session_hook_inputs() -> (ProcessHookInputs, DisabledHooks) {
    ProcessHookInputs::read_with_disabled(managed_settings(), crate::claude_import::import_marker())
}

/// Every session startup and mid-session reload loads hooks through this function.
/// This is the one place that chooses which hook sources to load.
pub(crate) fn discover_hooks(
    inputs: &ProcessHookInputs,
    git_root: Option<&Path>,
    compat: &xai_grok_tools::types::compat::CompatConfig,
    trust: Trust,
) -> (xai_grok_hooks::discovery::HookRegistry, Vec<HookError>) {
    xai_grok_hooks::discovery::assemble_hooks(
        inputs.config_layers(),
        DiscoveryOptions {
            git_root,
            grok_home: inputs.grok_home(),
            home: inputs.home(),
            compat: compat.hooks(),
            claude_import: inputs.claude_import(),
            trust,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xai_grok_hooks::config::HookProvenance;
    use xai_grok_hooks::error::HookError;
    use xai_grok_hooks::event::HookEventName;

    /// Write `content` as `<dir>/requirements.toml`.
    fn write_requirements(dir: &Path, content: &str) {
        std::fs::write(dir.join("requirements.toml"), content).unwrap();
    }

    /// A temp policy layer pins hooks for `SessionStart`, `UserPromptSubmit`, and `PreToolUse`.
    /// It flows through the real requirements read (`hook_config_layers_at`) and the real assembly (`assemble_hooks`).
    /// All three register with `Requirements` provenance, the provenance the disable exemption keys on.
    #[test]
    fn requirements_layer_pins_hooks_with_requirements_provenance() {
        let system_dir = tempfile::tempdir().unwrap();
        write_requirements(
            system_dir.path(),
            r#"
[[hooks.SessionStart]]
[[hooks.SessionStart.hooks]]
type = "command"
command = "/opt/policy/pin-session-start.sh"
timeout = 5

[[hooks.UserPromptSubmit]]
[[hooks.UserPromptSubmit.hooks]]
type = "command"
command = "/opt/policy/pin-prompt-submit.sh"
timeout = 5

[[hooks.PreToolUse]]
matcher = "*"
[[hooks.PreToolUse.hooks]]
type = "command"
command = "/opt/policy/pin-pre-tool-use.sh"
timeout = 5
"#,
        );

        let layers = xai_grok_config::hook_config_layers_at(Some(system_dir.path()), None);
        assert_eq!(layers.len(), 1, "one requirements layer expected");
        let Some(layer) = layers.first() else {
            panic!("one requirements layer expected: {layers:?}");
        };
        assert_eq!(layer.provenance(), HookProvenance::Requirements);
        assert_eq!(layer.source_name(), "requirements/system");

        let compat = xai_grok_tools::types::compat::CompatConfig::default();
        let (registry, errors) = xai_grok_hooks::discovery::assemble_hooks(
            &layers,
            DiscoveryOptions {
                git_root: None,
                grok_home: None,
                home: None,
                compat: compat.hooks(),
                claude_import: xai_grok_config::ClaudeImport::NotImported,
                trust: Trust::Untrusted,
            },
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        for (event, command) in [
            (HookEventName::SessionStart, "pin-session-start.sh"),
            (HookEventName::UserPromptSubmit, "pin-prompt-submit.sh"),
            (HookEventName::PreToolUse, "pin-pre-tool-use.sh"),
        ] {
            let spec = registry
                .hooks_for(event)
                .iter()
                .find(|s| {
                    s.command_raw
                        .as_deref()
                        .is_some_and(|c| c.contains(command))
                })
                .unwrap_or_else(|| panic!("pinned {event} hook must register"));
            assert_eq!(
                spec.layer,
                HookProvenance::Requirements,
                "pinned {event} hook must carry requirements provenance"
            );
            assert!(
                spec.is_managed_policy(),
                "requirements provenance must classify as managed policy"
            );
            assert!(
                spec.name.starts_with("requirements/system:"),
                "provenance-prefixed name expected, got {}",
                spec.name
            );
        }
    }

    /// A realistic enterprise policy hooks shape parses and registers through the real path.
    /// The shape: command hooks with `timeout: 5`, `PreToolUse` with `matcher: "*"` and two hooks in one group, and matcher-less lifecycle groups.
    /// The two `PreToolUse` hooks are byte-identical, so both parse but content dedup registers one effective hook.
    #[test]
    fn enterprise_policy_hooks_shape_registers() {
        let system_dir = tempfile::tempdir().unwrap();
        write_requirements(
            system_dir.path(),
            r#"
[[hooks.SessionStart]]
[[hooks.SessionStart.hooks]]
type = "command"
command = "policy/hooks/bin/lifecycle-audit.sh"
timeout = 5

[[hooks.PreToolUse]]
matcher = "*"
[[hooks.PreToolUse.hooks]]
type = "command"
command = "policy/hooks/bin/pretooluse-audit.sh"
timeout = 5
[[hooks.PreToolUse.hooks]]
type = "command"
command = "policy/hooks/bin/pretooluse-audit.sh"
timeout = 5

[[hooks.UserPromptSubmit]]
[[hooks.UserPromptSubmit.hooks]]
type = "command"
command = "policy/hooks/bin/lifecycle-audit.sh"
timeout = 5
"#,
        );

        let layers = xai_grok_config::hook_config_layers_at(Some(system_dir.path()), None);
        assert_eq!(layers.len(), 1);

        // Parse level: the verbatim structure yields both PreToolUse handlers.
        let (specs, errors) = xai_grok_hooks::config::parse_hooks_from_config_layers(&layers);
        assert!(errors.is_empty(), "errors: {errors:?}");
        let pre_specs: Vec<_> = specs
            .iter()
            .filter(|s| s.event == HookEventName::PreToolUse)
            .collect();
        assert_eq!(
            pre_specs.len(),
            2,
            "the PreToolUse group's two hooks must both parse"
        );
        for spec in &pre_specs {
            assert_eq!(spec.configured_matcher.as_deref(), Some("*"));
            let matcher = spec.matcher.as_ref().expect("matcher '*' compiles");
            assert!(
                matcher.is_match("run_terminal_command") && matcher.is_match("Bash"),
                "matcher '*' must match every tool"
            );
            assert_eq!(spec.timeout_ms, 5000, "timeout 5s converts to 5000ms");
        }

        // Registry level through the real assembly: all three events register with requirements provenance
        // The byte-identical PreToolUse duplicate collapses to one effective hook
        let compat = xai_grok_tools::types::compat::CompatConfig::default();
        let (registry, errors) = xai_grok_hooks::discovery::assemble_hooks(
            &layers,
            DiscoveryOptions {
                git_root: None,
                grok_home: None,
                home: None,
                compat: compat.hooks(),
                claude_import: xai_grok_config::ClaudeImport::NotImported,
                trust: Trust::Untrusted,
            },
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
        for event in [
            HookEventName::SessionStart,
            HookEventName::UserPromptSubmit,
            HookEventName::PreToolUse,
        ] {
            let policy_hooks: Vec<_> = registry
                .hooks_for(event)
                .iter()
                .filter(|s| s.layer == HookProvenance::Requirements)
                .collect();
            assert!(
                !policy_hooks.is_empty(),
                "pinned {event} hook must register with requirements provenance"
            );
        }
        assert_eq!(
            registry
                .hooks_for(HookEventName::PreToolUse)
                .iter()
                .filter(|s| s.layer == HookProvenance::Requirements)
                .count(),
            1,
            "byte-identical duplicate collapses under content dedup"
        );
    }

    #[test]
    fn directory_at_cursor_hooks_json_does_not_load_child_hooks() {
        let root = tempfile::tempdir().unwrap();
        let disguised = root.path().join(".cursor").join("hooks.json");
        std::fs::create_dir_all(&disguised).unwrap();
        std::fs::write(
            disguised.join("startup.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"cursor_hooks_json_dir_probe.sh"}]}]}}"#,
        )
        .unwrap();

        let compat = xai_grok_tools::types::compat::CompatConfig::default();
        let (registry, errors) = xai_grok_hooks::discovery::assemble_hooks(
            &[],
            DiscoveryOptions {
                git_root: Some(root.path()),
                grok_home: None,
                home: None,
                compat: compat.hooks(),
                claude_import: xai_grok_config::ClaudeImport::NotImported,
                trust: Trust::Trusted,
            },
        );

        assert!(
            !registry.all_hooks().iter().any(|h| {
                h.command_raw
                    .as_deref()
                    .is_some_and(|c| c.contains("cursor_hooks_json_dir_probe"))
            }),
            "child JSON under a directory at .cursor/hooks.json must not load as hooks"
        );
        assert!(
            errors.iter().any(|e| matches!(
                e,
                HookError::ReadFile { path, .. } if *path == disguised
            )),
            "reading the disguised directory as a settings file must surface ReadFile; got {errors:?}"
        );
    }
}
