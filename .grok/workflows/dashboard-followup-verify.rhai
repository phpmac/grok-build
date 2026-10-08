let meta = #{
    name: "dashboard-followup-verify",
    description: "复查 Dashboard workflow 状态修复的第二批改动: diff, 语义与回归面, 并发上限 3",
    phases: [
        #{ title: "Verify", detail: "3 路并行: diff 编译安全 / 行为语义 / 回归面" },
    ],
};

let verify_schema = #{
    "type": "object", "required": ["pass", "issues"],
    "properties": #{
        "pass": #{ "type": "boolean" },
        "issues": #{ "type": "array", "items": #{ "type": "string" } },
    },
};

let repo = "/Users/a/Downloads/grok-build";
let common = "";
common += "仓库: " + repo + " (分支 main, 只读当前分支, 禁止 checkout/commit/push). ";
common += "硬规则: 禁止执行任何 cargo 命令 (本仓禁止本地构建); 必须用 grep/read_file/git diff 只读手段看真实代码, 禁止凭记忆回答; 发现问题写 issues (带 file:line), 全清才 pass=true. ";
common += "本轮改动背景: (1) Dashboard 停止计划纳入可停止 workflow, 经 Effect::SendPrompt 发 /workflow stop <name>; (2) Rewind/ClearPrompt 的 busy 守卫纳入 active workflow 与 running bg task; (3) peek 状态行对背景工作报 Working; (4) shell 侧 SessionHandle 共享 workflow tracker Arc, resident_activity 与 turn-end delta (acp_agent.rs end_activity) 对 Active run 报 Working, 走 roster wire 让其他进程的 Dashboard 也能显示; (5) pager can_stop 排除 budget_limited, 对齐 shell accepts(Stop); (6) V1 overlay [stop] 尾部在关闭前先停背景工作. 改动共 16 个文件, 用 git diff 查看. ";
common += "上一轮复查的 5 个问题已修复: budget_limited 口径, V1 overlay 停止, end_activity delta, 新增守卫/overlay 用例, tracker 注释指路. 本轮重点复核这些修复本身是否正确与自洽. ";

phase("Verify");

let vjobs = [];

vjobs.push(#{
    prompt: common + "任务: 逐行审 git diff 的编译安全与测试正确性. "
        + "重点核对: (1) SessionHandle 新字段 workflow_tracker 的 3 个构造点 (spawn.rs, agent/mvp_agent/tests.rs, agent/subagent/tests/mod.rs) 是否全部初始化, 有无第 4 处遗漏 (全仓 grep SessionHandle 核实); "
        + "(2) DashboardStopPlan 新字段 workflow_names 的构造与 is_empty/would_stop_anything 是否自洽; stop_top_level_activity 新签名 (agent_id, agent) 的 3 个调用点是否都改了; "
        + "(3) Effect::SendPrompt 字段类型与构造是否匹配 (agent_id: AgentId, session_id: acp::SessionId, prompt_id: String, skill_token_ranges: Vec); uuid 依赖是否存在于 xai-grok-pager/Cargo.toml; "
        + "(4) 新增测试的 fixture (workflow_run_snapshot, row_activity_tests 拆分, shell 的 start_run 调用) 字段与方法是否真实存在且断言方向正确; "
        + "(5) peek.rs 的 super::row::has_background_work 可见性. 逐条给结论. ",
    label: "verify:diff",
    capability_mode: "read-only",
    output_schema: verify_schema,
});

vjobs.push(#{
    prompt: common + "任务: 对抗式复核行为语义. "
        + "(1) stop 链路: dashboard_stop_readiness 对 workflow-only 会话现在应判 Stoppable; V1 (workspace_dashboard_enabled=false) 与 V2 两条分支里 stop 键是否都会走到 stop_top_level_activity 并发出 /workflow stop; "
        + "发送的文本 /workflow stop <name> 与 Tasks 面板 panes.rs 的既有链路是否同一形态, shell 侧 slash_commands 能否接受 (查 xai-grok-shell 的 workflow slash 处理); "
        + "(2) can_stop 口径: management_available && is_active, paused/budget_limited/终态是否都被排除; "
        + "(3) app_view.rs 守卫: armed 的 ClearPrompt/RewindShowPicker 在 workflow/bg task 运行时被丢弃 (pending_action = None), 语义是否与注释一致, 有没有误伤正常路径; "
        + "(4) peek: extract_last_response_type 在回合空闲但有背景工作时提前返回 Working, 现有测试 (dispatch/tests/dashboard.rs 与 dispatch/tests/router.rs 的 peek_label 系列) 是否全部不受影响; "
        + "(5) shell: resident_activity 的优先级 NeedsInput > Working > live state 是否保持; has_active_run 在 restore 后无 stale Active (store.rs:108 附近把 persisted Active 改写为 Interrupted, 请核实); "
        + "tracker Arc 从 spawn 共享给 handle 与 manager 后, 有无生产代码会整体替换 manager.tracker 导致 handle 指向旧 tracker (区分测试代码与生产代码); "
        + "(6) pager 侧 roster_activity_to_state 对 Working 的映射是否让 append_roster_rows 的跨进程行进 Working 组. ",
    label: "verify:语义",
    capability_mode: "read-only",
    output_schema: verify_schema,
});

vjobs.push(#{
    prompt: common + "任务: 回归面清查 (只报告, 禁止改代码). "
        + "(1) 全仓 grep 引用被改函数/结构的测试与调用方: classify_top_level, has_background_work, extract_last_response_type, dashboard_stop_readiness, DashboardStopPlan, resident_activity, stoppable_activity_running, arm_dashboard_stop; 判断哪些既有断言会被本轮改动打破 (上一轮已同步 row_activity_tests.rs, 检查有没有漏网); "
        + "(2) tasks_pane/dock/workflows_overlay 的既有测试是否依赖旧签名或旧计数; "
        + "(3) 死锁面: resident_activity (mvp_agent 侧) 锁 workflow_tracker 的场景与 manager/actor 锁同一 tracker 的场景是否可能同线程重入或嵌套互锁; "
        + "(4) roster wire 序列化: RosterActivity 未改枚举, 确认 leader 协议与 roster_merge 不受影响; "
        + "(5) 效果审计: stop 键对 workflow-only 会话从静默变为发用户可见的 /workflow stop 提示词 (会话 scrollback 会回显该消息), 确认没有别的更隐蔽的调用路径被这一行为变化波及 (例如 Archive 流程会先 stop 再归档的用例). ",
    label: "verify:回归面",
    capability_mode: "read-only",
    output_schema: verify_schema,
});

let verdicts = parallel(vjobs);

let ok_count = 0;
let all_issues = [];
let j = 0;
for v in verdicts {
    if v != () && v.success && v.output != () && v.output.pass == true {
        ok_count += 1;
    } else {
        if v != () && v.success && v.output != () && v.output.issues != () {
            for it in v.output.issues { all_issues.push("verify 第 " + (j + 1).to_string() + " 路: " + it); }
        } else {
            all_issues.push("verify 第 " + (j + 1).to_string() + " 路失败或无输出, 视为未通过");
        }
    }
    j += 1;
}

if ok_count == 3 {
    log("复查 3/3 通过");
    complete(#{ summary: "4 项修复全部通过 3 路复查", pass: true });
} else {
    log("复查未全过: " + ok_count.to_string() + "/3");
    complete(#{ summary: "复查发现问题, 详见 verify_issues", pass: false, verify_issues: all_issues });
}
