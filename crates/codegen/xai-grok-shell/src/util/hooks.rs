use std::path::{Path, PathBuf};

use xai_grok_config::resolve_global_hook_sources;
use xai_grok_hooks::discovery::HookSource;
use xai_grok_hooks::error::HookError;
use xai_grok_workspace::HookSourceConfig;

/// Owned hook sources. Callers borrow via `as_sources()`.
pub(crate) struct HookSourcePaths {
    pub global: Vec<HookSourceConfig>,
    pub project: Vec<HookSourceConfig>,
}

impl HookSourcePaths {
    /// Borrow as `HookSource` refs. Project sources are excluded when untrusted.
    pub(crate) fn as_sources(
        &self,
        include_project: bool,
    ) -> (Vec<HookSource<'_>>, Vec<HookSource<'_>>) {
        let global = self
            .global
            .iter()
            .map(HookSourceConfig::as_hook_source)
            .collect();
        let project = if include_project {
            self.project
                .iter()
                .map(HookSourceConfig::as_hook_source)
                .collect()
        } else {
            vec![]
        };
        (global, project)
    }
}

/// Vendor settings-file paths are classified at the call site so a directory there cannot load as a hook dir.
fn classify_grok_hook_source(path: PathBuf) -> HookSourceConfig {
    if path.is_dir() {
        HookSourceConfig::Directory(path)
    } else {
        HookSourceConfig::SettingsFile(path)
    }
}

fn include_claude_hooks(compat: &xai_grok_tools::types::compat::CompatConfig) -> bool {
    compat.claude.hooks
        && !crate::claude_import::is_claude_import_marked_with_log("discover_hook_source_paths")
}

fn include_cursor_hooks(compat: &xai_grok_tools::types::compat::CompatConfig) -> bool {
    compat.cursor.hooks
}

/// Global and project hook source paths.
/// The registry file is never a discovery source; Claude and Cursor sources are appended when their compat gates are on.
/// Vendor settings paths are pushed as settings files here and probed for presence by xai_grok_workspace::folder_trust::repo_configs_present; a new vendor path needs both.
pub(crate) fn discover_hook_source_paths(
    git_root: Option<&Path>,
    compat: &xai_grok_tools::types::compat::CompatConfig,
) -> HookSourcePaths {
    let grok = xai_grok_config::user_grok_home();
    let home = xai_dirs::home_dir();
    let include_claude = include_claude_hooks(compat);
    let include_cursor = include_cursor_hooks(compat);

    // An unreadable hooks-paths file keeps the fixed Grok sources; a hard resolve failure omits all Grok global sources
    let mut global: Vec<HookSourceConfig> =
        match resolve_global_hook_sources(grok.as_deref(), /* reject_symlinks */ false) {
            Ok(resolved) => {
                if let Some(e) = &resolved.configured_error {
                    tracing::warn!(
                        error = %e,
                        "hooks-paths unreadable; retaining fixed Grok hook discovery sources only"
                    );
                }
                resolved
                    .discovery_sources()
                    .map(|s| classify_grok_hook_source(s.path.clone()))
                    .collect()
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "global hook source resolve hard-failed; omitting Grok global sources"
                );
                Vec::new()
            }
        };

    if let Some(h) = home.as_deref() {
        if include_claude {
            global.push(HookSourceConfig::SettingsFile(
                h.join(".claude").join("settings.json"),
            ));
            global.push(HookSourceConfig::SettingsFile(
                h.join(".claude").join("settings.local.json"),
            ));
        }
        if include_cursor {
            global.push(HookSourceConfig::SettingsFile(
                h.join(".cursor").join("hooks.json"),
            ));
        }
    }

    let mut project = Vec::new();
    if let Some(root) = git_root {
        if include_claude {
            project.push(HookSourceConfig::SettingsFile(
                root.join(".claude").join("settings.json"),
            ));
            project.push(HookSourceConfig::SettingsFile(
                root.join(".claude").join("settings.local.json"),
            ));
        }
        project.push(classify_grok_hook_source(root.join(".grok").join("hooks")));
        if include_cursor {
            project.push(HookSourceConfig::SettingsFile(
                root.join(".cursor").join("hooks.json"),
            ));
        }
    }

    HookSourcePaths { global, project }
}

/// Single load entry point: build compat-aware sources, gate project sources on trust, then load.
/// Every session-startup and mid-session reload site routes through here so the source policy stays in one place.
pub(crate) fn discover_hooks(
    git_root: Option<&Path>,
    compat: &xai_grok_tools::types::compat::CompatConfig,
    trusted: bool,
) -> (xai_grok_hooks::discovery::HookRegistry, Vec<HookError>) {
    // Read fresh each call (not cached): a mid-session `/hooks` reload must see an updated `config.toml` or `managed_config.toml`
    // This is lighter than `ConfigLayers::load` (only the small per-layer files, no campaigns, version overrides, or MDM)
    let config_layers = xai_grok_config::hook_config_layers();
    assemble_hooks(&config_layers, git_root, compat, trusted)
}

/// Pure, injectable core: combine config-layer hooks with file-source hooks and dedup once. Config-layer specs go first.
/// The first-wins dedup in [`xai_grok_hooks::discovery::registry_from_specs_deduped`] then lets a config hook beat a byte-identical file hook.
/// `config_layers` is a parameter (not read here) so tests can drive it with hand-built layers.
pub(crate) fn assemble_hooks(
    config_layers: &[xai_grok_config::HookConfigLayer],
    git_root: Option<&Path>,
    compat: &xai_grok_tools::types::compat::CompatConfig,
    trusted: bool,
) -> (xai_grok_hooks::discovery::HookRegistry, Vec<HookError>) {
    let (mut specs, mut errors) =
        xai_grok_hooks::config::parse_hooks_from_config_layers(config_layers);

    let source_paths = discover_hook_source_paths(git_root, compat);
    let (global_sources, project_sources) = source_paths.as_sources(trusted);
    let (file_specs, file_errors) =
        xai_grok_hooks::discovery::collect_specs_from_sources(&global_sources, &project_sources);
    specs.extend(file_specs);
    errors.extend(file_errors);

    (
        xai_grok_hooks::discovery::registry_from_specs_deduped(specs),
        errors,
    )
}

/// 收集插件贡献的 hook specs (文件 hooks.json + manifest inline hooks).
///
/// 会话启动路径与 reload 路径共用: 此前只有 reload 路径
/// (`apply_plugin_registry_snapshot`) 挂插件 hooks, 冷启动会话的 hook
/// 注册表恒不含它们, 插件拦截静默失效.
/// specs 名带 `plugin/` 前缀, reload 时按前缀整体移除后重挂, 不产生重复.
pub(crate) fn collect_plugin_hook_specs(
    plugins: &[&xai_grok_agent::plugins::LoadedPlugin],
) -> Vec<xai_grok_hooks::config::HookSpec> {
    let mut specs = Vec::new();
    for plugin in plugins {
        if let Some(ref hooks_path) = plugin.hooks_path {
            let (mut parsed, warnings) =
                xai_grok_agent::plugins::hooks_adapter::parse_plugin_hooks(
                    hooks_path,
                    &plugin.name,
                    &plugin.root_str(),
                    &plugin.data_dir_str(),
                );
            for w in warnings {
                tracing::warn!("{w}");
            }
            specs.append(&mut parsed);
        }
        if let Some(ref inline_value) = plugin.inline_hooks {
            let (mut parsed, warnings) =
                xai_grok_agent::plugins::hooks_adapter::parse_plugin_hooks_from_value(
                    inline_value,
                    &plugin.name,
                    &plugin.root_str(),
                    &plugin.data_dir_str(),
                );
            for w in warnings {
                tracing::warn!("{w}");
            }
            specs.append(&mut parsed);
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;
    use xai_grok_hooks::config::HookProvenance;
    use xai_grok_hooks::event::HookEventName;

    /// 构造一个 LoadedPlugin 测试夹具, hooks 相关字段由调用方指定.
    fn loaded_plugin_fixture(
        name: &str,
        hooks_path: Option<std::path::PathBuf>,
        inline_hooks: Option<serde_json::Value>,
    ) -> xai_grok_agent::plugins::LoadedPlugin {
        use xai_grok_agent::plugins::discovery::PluginId;
        use xai_grok_agent::plugins::{PluginOrigin, PluginScope};
        let root = std::path::PathBuf::from(format!("/tmp/test-plugins/{name}"));
        xai_grok_agent::plugins::LoadedPlugin {
            name: name.to_string(),
            id: PluginId::new(PluginScope::User, &root, name),
            root: root.clone(),
            canonical_root: root.clone(),
            scope: PluginScope::User,
            origin: PluginOrigin::UserGrok,
            trusted: true,
            enabled: true,
            version: Some("1.0.0".to_string()),
            description: Some(format!("test plugin {name}")),
            skill_dirs: vec![],
            command_dirs: vec![],
            agent_dirs: vec![],
            hooks_path,
            mcp_config_path: None,
            lsp_config_path: None,
            skill_count: 0,
            agent_count: 0,
            skill_names: vec![],
            agent_names: vec![],
            has_hooks: hooks_path.is_some() || inline_hooks.is_some(),
            hook_count: 0,
            has_inline_hooks_only: hooks_path.is_none() && inline_hooks.is_some(),
            mcp_server_count: 0,
            has_inline_mcp_only: false,
            lsp_server_count: 0,
            has_inline_lsp_only: false,
            inline_hooks,
            inline_mcp_servers: None,
            inline_lsp_servers: None,
            conflict: None,
        }
    }

    /// 启动路径回归: 插件文件 hooks 必须被收集, 名带 `plugin/` 前缀
    /// (reload 的 remove_by_prefix 依赖该前缀去重), 且能装入注册表.
    #[test]
    fn plugin_file_hooks_are_collected_for_startup_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(
            hooks_dir.join("hooks.json"),
            r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"python3 pre.py"}]}]}}"#,
        )
        .unwrap();

        let plugin = loaded_plugin_fixture(
            "hookify",
            Some(hooks_dir.join("hooks.json")),
            None,
        );
        let specs = collect_plugin_hook_specs(&[&plugin]);
        assert_eq!(specs.len(), 1, "one PreToolUse spec expected");
        assert!(
            specs[0].name.starts_with("plugin/hookify/"),
            "spec name must carry the plugin/ prefix for reload dedup, got {}",
            specs[0].name
        );
        assert_eq!(specs[0].event, HookEventName::PreToolUse);

        // 装入注册表后 PreToolUse 可见 (spawn 启动路径的等效断言).
        let mut registry = xai_grok_hooks::discovery::HookRegistry::default();
        registry.append_specs(specs);
        assert_eq!(
            registry.hooks_for(HookEventName::PreToolUse).len(),
            1,
            "startup registry must expose the plugin PreToolUse hook"
        );
    }

    /// 无 hooks 的插件不得贡献任何 spec (对其他插件零影响).
    #[test]
    fn plugin_without_hooks_contributes_no_specs() {
        let plugin = loaded_plugin_fixture("no-hooks", None, None);
        let specs = collect_plugin_hook_specs(&[&plugin]);
        assert!(specs.is_empty(), "expected no specs, got {specs:?}");
    }

    /// manifest inline hooks 与文件 hooks 一样被收集.
    #[test]
    fn plugin_inline_hooks_are_collected() {
        let inline = serde_json::json!({
            "hooks": {
                "PreToolUse": [{"hooks": [{"type": "command", "command": "inline.sh"}]}]
            }
        });
        let plugin = loaded_plugin_fixture("inline-only", None, Some(inline));
        let specs = collect_plugin_hook_specs(&[&plugin]);
        assert_eq!(specs.len(), 1, "inline PreToolUse spec expected");
        assert_eq!(specs[0].event, HookEventName::PreToolUse);
    }

    /// 多插件混合 (带 hooks / 不带 hooks) 只收集带 hooks 的; reload 语义下
    /// remove_by_prefix("plugin/") 再重挂不会累积重复.
    #[test]
    fn mixed_plugins_collect_only_hooked_ones_and_reload_dedups() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(
            hooks_dir.join("hooks.json"),
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"stop.py"}]}]}}"#,
        )
        .unwrap();

        let hooked = loaded_plugin_fixture("hookify", Some(hooks_dir.join("hooks.json")), None);
        let plain = loaded_plugin_fixture("notify", None, None);
        let specs = collect_plugin_hook_specs(&[&hooked, &plain]);
        assert_eq!(specs.len(), 1, "only the hooked plugin contributes");

        let mut registry = xai_grok_hooks::discovery::HookRegistry::default();
        registry.append_specs(specs.clone());
        // 模拟 reload: 先按前缀整体移除再重挂.
        registry.remove_by_prefix("plugin/");
        registry.append_specs(specs);
        assert_eq!(
            registry.hooks_for(HookEventName::Stop).len(),
            1,
            "re-appending after prefix removal must not duplicate hooks"
        );
    }

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
        assert_eq!(layers[0].provenance(), HookProvenance::Requirements);
        assert_eq!(layers[0].source_name(), "requirements/system");

        let compat = xai_grok_tools::types::compat::CompatConfig::default();
        let (registry, errors) = assemble_hooks(&layers, None, &compat, false);
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
        let (registry, errors) = assemble_hooks(&layers, None, &compat, false);
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
        let (registry, errors) =
            assemble_hooks(&[], Some(root.path()), &compat, /*trusted*/ true);

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
                HookError::ReadFile { path, .. } if path == &disguised
            )),
            "reading the disguised directory as a settings file must surface ReadFile; got {errors:?}"
        );
    }
}
