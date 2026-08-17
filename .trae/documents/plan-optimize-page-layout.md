# SecretBox 页面布局优化计划

## Summary

针对用户截图反馈的 4 个布局问题进行优化:
1. 左侧条目列表内容多时缺少滚动条
2. 主页面"保存/删除"按钮悬浮在内容上,与下方内容重叠
3. "保密内容"文本框固定 240px 过高,小内容时浪费空间且不能自动适应
4. "备注"文本框空间利用率低,需要自动适应

**改动范围:** 仅前端 ( `web/style.css` + `web/app.js` ) ,后端无任何改动。

---

## Current State Analysis

### 关键现状 (从 `web/style.css` 提取)

| 选择器 | 当前值 | 问题 |
|---|---|---|
| `.item-list` (L299) | `overflow-y: auto; flex: 1;` | flex 子项缺 `min-height: 0`,在某些高度下会撑出 sidebar 而不滚动 |
| `.editor` (L340-351) | `overflow-y: auto; max-height: calc(100vh - 120px);` | 与父级 `.content` 双重滚动 |
| `.editor-actions` (L543-554) | `position: sticky; bottom: 0; background: var(--bg);` | 悬浮于编辑器底部,与下方"历史版本"视觉重叠 |
| `#itemValue` (L521-525) | `min-height: 240px; max-height: 60vh;` | 最小高度过大,短内容浪费空间 |
| `.note-textarea` (L497-500) | `min-height: 80px; max-height: 200px;` | 最小高度偏大,无内容时仍占 80px |

### 视觉证据(用户截图)
- **图1**:左侧栏 "deepseek key" 在底部被截断,**没有滚动条出现**;右侧 "保存/删除" 按钮悬浮在内容上,底部露出一截"历史版本"卡片。
- **图2**:放大图,清楚看到操作按钮(蓝/红)与"备注"卡片、"历史版本" 标题**相互重叠**。

---

## Proposed Changes

### 1) 修复左侧栏滚动条 — `web/style.css`

**问题根因:** `.item-list` 是 flex 列布局的子项,默认 `min-height: auto` 会让内容自然高度把容器撑大,导致 `overflow-y: auto` 失效。

**改动 (`.item-list` L299):**
```css
.item-list {
  list-style: none;
  overflow-y: auto;
  flex: 1;
  min-height: 0;          /* 新增:允许 flex 子项收缩,触发滚动 */
  padding: 8px;
}
```

**滚动条微调 (`:-webkit-scrollbar-thumb` L137 附近):** 把宽度从 8px 改到 10px,thumb 颜色用更明显的 `--dim` 而非 `--border-strong`,确保浅/深主题下都清晰可见。
```css
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb {
  background: var(--dim);
  border-radius: 5px;
}
::-webkit-scrollbar-thumb:hover { background: var(--text-soft); }
```

---

### 2) 修复操作按钮悬浮 — `web/style.css` + `web/index.html`(无需结构改动)

**问题根因:** `.editor-actions` 用了 `position: sticky; bottom: 0`,在双滚动容器中表现为悬浮且与历史版本重叠。

**改动 A — 移除编辑器自身的滚动 (`web/style.css` `.editor` L340-351):**
```css
.editor {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;                          /* 原 20px,微调间距更紧凑 */
  max-width: 900px;
  margin: 0 auto;
  width: 100%;
  /* 删除: overflow-y: auto; max-height; padding-bottom: 10px; */
}
```

**改动 B — 移除按钮 sticky (`web/style.css` `.editor-actions` L543-554):**
```css
.editor-actions {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
  padding-top: 4px;
  /* 删除: position: sticky; bottom: 0; background; padding-bottom: 8px; z-index */
}
```

**效果:** 整个 `.content` 区域只滚动一次,按钮在自然流中出现在"备注"卡片之后,完全避免重叠。

---

### 3) 保密内容自动适应大小 — `web/style.css` + `web/app.js`

**改动 A — 缩小最小高度 (`web/style.css` `#itemValue` L521-525):**
```css
#itemValue {
  width: 100%;
  min-height: 72px;       /* 原 240px */
  max-height: 60vh;
  /* 其余保持不变 */
}
```

**改动 B — 新增自动高度调整函数 (`web/app.js`):**
在文件中(约 L219 `toggleValueBtn` 附近)新增工具函数:
```js
function autoResizeTextarea(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight, el.clientHeight || Infinity) + 'px';
}
```

**绑定点 (app.js):**
- `itemValueEl.addEventListener('input', () => autoResizeTextarea(itemValueEl))`
- `noteTextarea.addEventListener('input', () => autoResizeTextarea(noteTextarea))`
- 在 `showEditor(it)` (L439) 和 `createNew()` (L468) 末尾各加一次调用,让切换/新建条目时立刻按内容调整。

**为 `#itemNote` 添加 id 引用:** 当前在 `app.js` 中通过 `$('#itemNote')` 访问,可直接用,无需改 HTML。

---

### 4) 备注栏空间利用率 — `web/style.css`

**改动 (`.note-textarea` L497-500):**
```css
.note-textarea {
  width: 100%;
  min-height: 44px;       /* 原 80px */
  max-height: 240px;      /* 原 200px,稍微增加上限以便长备注 */
  /* 其余保持不变 */
}
```

配合 (3) 的 `autoResizeTextarea` 函数,空备注仅占 44px,有内容时自动撑高直到 240px。

---

## Files to Modify

| 文件 | 变更类型 |
|---|---|
| `web/style.css` | 修改 5 个选择器:`.item-list`、`:-webkit-scrollbar*`、`.editor`、`.editor-actions`、`#itemValue`、`.note-textarea` |
| `web/app.js` | 新增 `autoResizeTextarea` 函数;在 4 处调用(2 个 input 监听 + `showEditor` + `createNew`) |

后端 Go 文件 (`main.go` / `handlers.go` / `db.go` / `crypto.go`) **均不改动**。

---

## Assumptions & Decisions

1. **保留操作按钮始终可见的体验:** 不再用 sticky,而是依赖 `.content` 的整体滚动;用户滚动到末尾自然看到按钮,这是当前代码原本 sticky 想达到的"随时能保存"目标的简化替代。
2. **不引入第三方依赖:** 自写极简 `autoResizeTextarea` 函数,无 `autosize.js` 等依赖。
3. **不改变设计语言:** 仅调尺寸/定位/滚动,不修改主题、字体、颜色。
4. **响应式断点不重做:** 改动对各断点 (980/720/480) 兼容,因为只是缩减 min-height 与移除 sticky。

---

## Verification Steps

1. **左侧栏滚动**
   - 启动 `go run .` (或直接 `secretbox.exe`),浏览器打开 `http://127.0.0.1:8080`。
   - 左侧条目超过可视高度时,**应看到 10px 宽的滚动条**,且能上下滚动到所有条目。

2. **操作按钮不再悬浮**
   - 选中任一条目,滚动至底部。
   - "保存 / 删除" 按钮应**紧跟在"备注"卡片下方**,与"历史版本"之间有清晰间距,无视觉重叠。

3. **保密内容自适应**
   - 选一条内容仅 1 行的条目,文本框高度应**接近一行 + 内边距**(约 70-80px),不再固定 240px。
   - 选一条多行内容条目,文本框随内容增长,直到 `max-height: 60vh`。
   - 手动输入更多内容,文本框实时跟随增高。

4. **备注自适应**
   - 清空备注,文本框高度约 44px。
   - 粘贴多行备注,文本框自动撑高直至 240px 上限。

5. **回归检查**
   - 新增 / 保存 / 删除 / 还原版本 / 主题切换 / 响应式 (480/720/980) 全部功能正常。
