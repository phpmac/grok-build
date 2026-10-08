let meta = #{
    name: "dashboard-working-workflow-fix",
    description: "Dashboard 的 Working 筛选漏掉运行中 workflow 的会话: 3 路定位, 1 路修复, 3 路复查, 并发上限 3",
    phases: [
        #{ title: "Locate", detail: "并行验证状态链, workflow 语义与测试落点" },
        #{ title: "Fix", detail: "实施最小修复与单元测试" },
        #{ title: "Verify", detail: "并行复查 diff, 语义与回归面" },
    ],
};

let locate_schema = #{
    "type": "object", "required": ["confirmed", "evidence", "notes"],
    "properties": #{
        "confirmed": #{ "type": "boolean" },
        "evidence": #{ "type": "array", "items": #{ "type": "object", "required": ["file", "fact"], "properties": #{ "file": #{ "type": "string" }, "line": #{ "type": "integer" }, "fact": #{ "type": "string" } } } },
        "notes": #{ "type": "array", "items": #{ "type": "string" } },
    },
};
let fix_schema = #{
    "type": "object", "required": ["changed_files", "summary", "test_added"],
    "properties": #{
        "changed_files": #{ "type": "array", "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
        "test_added": #{ "type": "string" },
    },
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
common += "仓库: " + repo + " (当前分支 main, 只看当前分支, 禁止 checkout/commit/push). ";
common += "硬规则: 禁止执行任何 cargo 命令 (本仓禁止本地构建, 会生成 target/); 必须用 grep/read_file/git diff 等只读手段看真实代码, 禁止凭记忆回答. ";
common += "背景: Agent Dashboard 按会话状态分组并提供状态筛选, Working 组/Working 筛选由 classify_top_level 决定. 用户报告: 会话回合已结束但有 workflow 仍在运行时, 该会话不出现在 Working 筛选里. ";

phase("Locate");

let jobs = [];

jobs.push(#{
    prompt: common + "任务: 验证状态判定链是否真的漏掉 workflow. "
        + "读 crates/codegen/xai-grok-pager/src/views/dashboard/row.rs 的 classify_top_level (约 381 行) 与 has_background_work (约 395 行), "
        + "读 src/views/dashboard/row_activity.rs 的 has_live_parent_activity (约 10 行), "
        + "读 src/app/agent_view/mod.rs 的 workflow_runs 字段 (约 912 行) 与 src/views/workflows.rs 的 WorkflowRunSnapshot::is_active (约 56 行). "
        + "逐条确认: (1) classify_top_level 判 Working 只看 has_live_parent_activity 与 has_background_work; "
        + "(2) has_background_work 只看 session.bg_tasks 与 scheduled_tasks, 完全没有读 agent.workflow_runs; "
        + "(3) 因此回合空闲但有 active workflow 的本地会话会被判成 Idle. "
        + "再用 grep 列出 classify_top_level 与 has_background_work 的全部调用方及行号 (含 app/dispatch/dashboard.rs, app/workspace_sync.rs, tests). "
        + "confirmed=true 仅当你亲眼读到上述代码. evidence 每条写 file+line+fact. ",
    label: "locate:状态链",
    capability_mode: "read-only",
    output_schema: locate_schema,
});

jobs.push(#{
    prompt: common + "任务: 确定 workflow 运行状态的正确判活谓词. "
        + "读 src/app/acp_handler/workflow_ingest.rs, 弄清 WorkflowRunSnapshot 如何插入/更新/清除, status 字段会出现哪些取值 (active, paused 系列, budget_limited, complete, failed, cancelled, interrupted, cleared 等), 各在什么事件下写入. "
        + "读 src/views/workflows.rs 的 is_active/is_terminal/can_pause. "
        + "读 src/views/tasks_pane.rs 里对 run.is_active() 的计数逻辑 (约 status_counts_from). "
        + "回答: 判断 workflow 仍在干活, 用 status == active (即 is_active()) 是否与 Tasks 面板的 running 计数一致; paused 与 budget_limited 是否应排除; 已 terminal 的值有哪些. "
        + "把每个结论写进 evidence (file+line+fact), notes 写状态值清单. confirmed=true 表示 is_active() 足以作为 Working 判据. ",
    label: "locate:workflow语义",
    capability_mode: "read-only",
    output_schema: locate_schema,
});

jobs.push(#{
    prompt: common + "任务: 摸清测试落点, 为修复做准备. "
        + "读 src/views/dashboard/row.rs 内的测试模块 (约 1560-1800 行), 找 classify_top_level 对 bg_tasks/scheduled_tasks 的现有断言写法. "
        + "找测试里构造 AgentView 的辅助函数 (src/views/dashboard/test_support.rs, app/dispatch/tests/dashboard.rs 的 test_agent 等). "
        + "找 src/views/tasks_pane.rs 测试里 make_workflow_run 之类构造 WorkflowRunSnapshot 的现成写法 (注意 received_at 用 std::time::Instant). "
        + "回答: 新增单测 (空闲会话 + active workflow 必须判 Working; 终态 workflow 必须保持 Idle) 应放在哪个文件, 复用哪个构造器, 需要哪些字段. "
        + "notes 里写: 该测试目标 (xai-grok-pager lib tests) 是否属于 CI 覆盖范围. confirmed=true 表示测试落点已查清. ",
    label: "locate:测试落点",
    capability_mode: "read-only",
    output_schema: locate_schema,
});

let located = parallel(jobs);

let chain_ok = false;
let i = 0;
for r in located {
    if r != () && r.success && r.output != () && r.output.confirmed == true {
        if i == 0 { chain_ok = true; }
    }
    if r == () || !r.success {
        log("locate 第 " + (i + 1).to_string() + " 路失败或无输出");
    }
    i += 1;
}
if chain_ok == false {
    complete(#{ summary: "定位失败: 未能用代码证据确认 classify_top_level 漏看 workflow_runs, 按约定不修改代码. ", fixed: false });
}
log("定位完成: 3 路证据齐, 进入修复");

phase("Fix");

let fix_prompt = "";
fix_prompt += "仓库: " + repo + ". 你负责实施最小修复. 硬规则: 禁止任何 cargo 命令 (本仓禁止本地构建); 禁止 commit/push/切分支; 注释用中文且标点用英文; diff 保持最小. ";
fix_prompt += "问题: crates/codegen/xai-grok-pager/src/views/dashboard/row.rs 的 has_background_work (约 395 行) 只看 session.bg_tasks 与 scheduled_tasks, 会话有 active workflow 时 (agent.workflow_runs, 见 src/app/agent_view/mod.rs 约 912 行; is_active 见 src/views/workflows.rs 约 56 行) 不判 Working, Dashboard 的 Working 组与 Working 筛选因此漏掉该会话. ";
fix_prompt += "步骤: (1) 先读 row.rs 的 classify_top_level/has_background_work, row_activity.rs, 现有测试 (约 1560-1800 行) 与 src/views/tasks_pane.rs 测试里的 WorkflowRunSnapshot 构造写法; (2) 修改 has_background_work: 追加 agent.workflow_runs 中存在 is_active() 为 true 的项这一条件; (3) 在同文件测试模块加两个单测: 空闲会话 + 一个 active workflow 判 Working; 空闲会话 + 一个 complete (终态) workflow 保持 Idle; 构造方式照抄现有测试与 make_workflow_run 风格, received_at 用 std::time::Instant::now(); (4) 只改 row.rs, 除非构造测试确需改测试辅助文件. ";
fix_prompt += "完成后报告: changed_files, summary (改了什么, 一句), test_added (新测试名字与断言内容). ";
let fix_result = agent(fix_prompt, #{
    label: "fix:has_background_work",
    capability_mode: "read-write",
    output_schema: fix_schema,
});
if fix_result == () || !fix_result.success {
    complete(#{ summary: "修复代理失败, 未产出可信修改. ", fixed: false });
}
log("修复完成: " + fix_result.output.summary);

phase("Verify");

let vjobs = [];

vjobs.push(#{
    prompt: common + "任务: 复查未提交 diff 的正确性. "
        + "在仓库里跑 git diff (只看工作区未提交改动), 逐行审: (1) 改动是否只落在 row.rs (或修复报告所列文件); (2) has_background_work 新条件的写法是否编译安全 (agent.workflow_runs 类型为 Vec<WorkflowRunSnapshot>, is_active 为 &self 方法, 借用与迭代器写法无误, 未引入新依赖); (3) 测试是否照现有风格构造数据, 断言方向正确. "
        + "发现问题就写进 issues, 没问题 pass=true. 必须真的跑 git diff 并读改动后的文件, 禁止只复述修复报告. ",
    label: "verify:diff",
    capability_mode: "read-only",
    output_schema: verify_schema,
});

vjobs.push(#{
    prompt: common + "任务: 对抗式复核判活语义. "
        + "读改后的 has_background_work/classify_top_level, src/views/workflows.rs 的 is_active/is_terminal, src/app/acp_handler/workflow_ingest.rs 的状态写入点, src/views/tasks_pane.rs 的 running 计数. "
        + "逐一证伪: (1) paused 与 budget_limited 的 workflow 是否被正确排除在 Working 外; (2) 终态 (complete/failed/cancelled/interrupted) 是否不会误判 Working; (3) 会话有待批权限时是否仍优先 NeedsInput, 顺序没被破坏; (4) 与 Tasks 面板的 running 计数是否同一口径. "
        + "任何一条不成立都写进 issues; 全部成立才 pass=true. ",
    label: "verify:语义",
    capability_mode: "read-only",
    output_schema: verify_schema,
});

vjobs.push(#{
    prompt: common + "任务: 同类问题与回归面清查 (只报告, 禁止改代码). "
        + "(1) app/workspace_sync.rs 约 179 行用 classify_top_level 派生 workspace store 行状态, 确认修复后它自动一致; (2) grep 其他自行判断 Working/活动 的地方 (views/agent_status.rs, views/dock, views/dashboard/peek.rs, preview.rs, row_activity.rs 的徽标), 看有没有同类漏算 workflow 的口子, 有就写进 issues 并注明 file:line; (3) roster_activity_to_state 只服务未加载的跨进程会话, workflow 快照只在本进程, 说明该边界; (4) 检查现有引用 has_background_work/classify_top_level 的测试逻辑上是否仍成立. "
        + "同类漏算只报告不修. 全清 pass=true. ",
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
    complete(#{ summary: "问题已确认并修复: has_background_work 现在把 active workflow 计为 Working; 3 路复查全过. 编译与测试验证留待 CI (本仓禁止本地 cargo). ", fixed: true, changed_files: fix_result.output.changed_files, test_added: fix_result.output.test_added, verify_issues: all_issues });
} else {
    log("复查未全过: " + ok_count.to_string() + "/3");
    complete(#{ summary: "修复已写入但复查发现问题, 详见 verify_issues. ", fixed: false, verify_issues: all_issues });
}
