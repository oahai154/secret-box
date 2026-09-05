# 07 — 界面对照验收（美术零降级走查）

**What to build:**
"美术不降级"承诺的正式验收关：Go 版 `web/` 与 Svelte 版在相同数据状态下逐页自动截图比对，差异逐个修掉，全部页面达到一致。覆盖状态包括：锁定页、列表页、空态、编辑态、历史版本页、设置页、各确认弹窗。

**Blocked by:** 03、04、05、06 — 全部界面功能齐备

**Status:** done

> 实施说明：
> - 基准矩阵扩展到 9 个状态：`capture-baselines.mjs`（npm run baseline，Go 版跑在同机同浏览器
>   同视口 1280x800 / 浅色 / reducedMotion）新增截取 value-visible（保密内容显示）、
>   history（历史版本面板滚动到视野）、settings（设置弹窗）、change-password（改密弹窗）、
>   db（数据备份弹窗）、empty（空列表态，逐个删除条目后）。
> - Svelte 侧对照在 `visual.spec.ts`（9 个测试）复现相同状态后像素对比，
>   阈值 1%，差异图写 test-results/。
> - 走查发现并修复的差异：弹窗类截图两侧背景状态不一致（Go 基准截弹窗时停留在
>   历史面板滚动位置且选中条目 2，Svelte 侧默认条目 1）导致 ~1% 差异。
>   修复 = 两侧截弹窗前统一"选中条目 1 + 全量滚动复位（window + 所有元素 scrollTop）"。
>   修复后差异从 0.8~1.0% 降至 0.02~0.04%。
> - 差异报告（最终，pixelmatch threshold 0.2）：auth 0.057% / main 0.040% / new 0.166% /
>   value-visible 0.040% / history 0.081% / settings 0.017% / change-password 0.027% /
>   db 0.038% / empty 0.000%。全部远低于 1% 阈值，剩余差异均为浏览器引擎字体渲染
>   抗锯齿差（判定为合理，无需修复）。
> - 确认/验证类弹窗（删除确认、主密码验证、还原确认）在 Go 版为原生 window.confirm/
>   自绘弹窗瞬态，基准截图不含弹窗态（#03 已记录），这些组件按 card-verify 美术基准
>   自审一致，不参与像素对比。
> - 纳入常规命令：`npm test`（全量 Playwright，含视觉矩阵）、`npm run visual`（仅视觉）、
>   `npm run baseline`（重生成 Go 基准，仅在 Go 版 UI 或基准流程变化时运行）。

- [x] 截图矩阵自动比对脚本覆盖上述全部页面与状态，输出差异报告
- [x] 全部页面像素对比在阈值内，差异报告归零或差异项均被判定为合理（如系统字体渲染差异）并记录
- [x] 对比脚本纳入常规测试命令，此后 UI 改动可随时回归
