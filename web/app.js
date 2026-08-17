/* SecretBox 前端逻辑 */
(function () {
  'use strict';

  const $ = (sel) => document.querySelector(sel);
  let token = null;
  let currentId = null;
  let items = [];

  // ---------- API 封装 ----------
  async function api(method, path, body) {
    const headers = { 'Content-Type': 'application/json' };
    if (token) headers['Authorization'] = 'Bearer ' + token;
    const res = await fetch(path, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || ('请求失败 ' + res.status));
    return data;
  }

  // ---------- Toast ----------
  let toastTimer;
  function toast(msg, type) {
    const el = $('#toast');
    el.textContent = msg;
    el.className = 'toast show ' + (type || '');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.remove('show'), 2300);
  }

  // ---------- 主题切换 ----------
  const THEME_KEY = 'secretbox-theme';
  function applyTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    try { localStorage.setItem(THEME_KEY, theme); } catch (_e) { /* 忽略存储失败 */ }
    document.querySelectorAll('.theme-opt').forEach((b) => {
      b.classList.toggle('active', b.dataset.themeOpt === theme);
    });
  }
  function initTheme() {
    const saved = (() => { try { return localStorage.getItem(THEME_KEY); } catch (_e) { return null; } })();
    applyTheme(saved === 'light' || saved === 'dark' || saved === 'system' ? saved : 'system');
  }
  document.getElementById('themeSwitch').addEventListener('click', (e) => {
    const btn = e.target.closest('.theme-opt');
    if (btn) applyTheme(btn.dataset.themeOpt);
  });
  // 系统主题变化时,若处于 "system" 模式则自动跟随
  window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
    if (document.documentElement.getAttribute('data-theme') === 'system') {
      applyTheme('system');
    }
  });

  // ---------- 数据备份 / 迁移 / 清除 ----------
  async function openDBModal() {
    renderDBInfo('加载中…');
    $('#dbModal').classList.remove('hidden');
  }
  async function renderDBInfo(prefix) {
    const box = $('#dbInfo');
    const base = prefix ? `<div class="db-row"><span class="db-label">状态</span><span class="db-value">${esc(prefix)}</span></div>` : '';
    try {
      const st = await api('GET', '/api/status');
      let count = 0;
      try { count = items.length; } catch (_e) {}
      box.innerHTML = base +
        `<div class="db-row"><span class="db-label">条目</span><span class="db-value">${count} 条</span></div>` +
        `<div class="db-row"><span class="db-label">主密码</span><span class="db-value">${st.has_password ? '✅ 已设置' : '未设置'}</span></div>`;
    } catch (_e) {
      box.innerHTML = base + `<div class="db-row"><span class="db-label">状态</span><span class="db-value">无法获取信息</span></div>`;
    }
  }

  // 导出:弹出迁移口令 → 调用后端 → 触发前端下载
  async function exportBackup() {
    const pw = prompt('设置迁移口令(至少 4 位)。该口令用于加密迁移文件,请务必牢记:');
    if (pw === null) return;
    if (pw.trim().length < 4) { toast('迁移口令至少 4 个字符', 'err'); return; }
    try {
      const data = await api('POST', '/api/export', { password: pw });
      // content 即迁移文件内容(base64 文本);直接保存为 .secretbox 文件,导入时原样上传
      const blob = new Blob([data.content], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = data.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast('已导出迁移文件: ' + data.filename);
    } catch (e) { toast('导出失败: ' + e.message, 'err'); }
  }

  // 导入:用户先选文件,后端用口令解密校验并还原
  async function handleImportFile(file) {
    if (!file) { toast('请选择迁移文件', 'err'); return; }
    const pw = prompt('输入该迁移文件的口令:');
    if (pw === null) return;
    const text = await file.text();
    try {
      const data = await api('POST', '/api/import', { password: pw, content: text });
      toast(`导入成功: ${data.items} 条数据`, 'ok');
      // 若导入内容设置了主密码,需用原主密码重新解锁
      token = null;
      currentId = null;
      items = [];
      configureAuthMode(data.has_password);
      showAuth();
      $('#authPassword').focus();
    } catch (e) {
      toast('导入失败: ' + e.message, 'err');
    }
    $('#importFile').value = '';
  }

  // 清除本地痕迹
  async function wipe() {
    if (!confirm('将删除本地全部数据(条目、历史、主密码)。除非你已导出迁移文件,否则此操作不可恢复。确定继续?')) return;
    try {
      await api('POST', '/api/wipe');
      toast('已清除本地全部数据', 'ok');
      token = null;
      currentId = null;
      items = [];
      configureAuthMode(false);
      showAuth();
      $('#authPassword').focus();
    } catch (e) { toast('清除失败: ' + e.message, 'err'); }
  }

  // ---------- 视图切换 ----------
  function showAuth() {
    $('#mainView').classList.add('hidden');
    $('#authView').classList.remove('hidden');
  }
  function showMain() {
    $('#authView').classList.add('hidden');
    $('#mainView').classList.remove('hidden');
  }

  // ---------- 认证流程 ----------
  async function init() {
    try {
      const st = await api('GET', '/api/status');
      if (st.unlocked) {
        showMain();
        try {
          await loadItems();
        } catch (unlockErr) {
          // 服务端已解锁但客户端无有效 token(会话不同步),回到解锁页
          token = null;
          showAuth();
          configureAuthMode(st.has_password);
        }
        return;
      }
      showAuth();
      configureAuthMode(st.has_password);
    } catch (e) {
      // 无法获取状态时,按未设置密码处理;若后续解锁失败,configureAuthMode 会按实际状态纠正
      showAuth();
      $('#authSubtitle').textContent = '无法连接服务: ' + e.message;
      configureAuthMode(false);
    }
  }

  function configureAuthMode(hasPassword) {
    const sub = $('#authSubtitle');
    const btn = $('#authBtn');
    const p2 = $('#authPassword2');
    if (hasPassword) {
      sub.textContent = '请输入主密码解锁';
      p2.classList.add('hidden');
      btn.textContent = '解锁';
      $('#authPassword').placeholder = '主密码';
    } else {
      sub.textContent = '首次使用,设置主密码以加密存储';
      p2.classList.remove('hidden');
      btn.textContent = '创建并进入';
      $('#authPassword').placeholder = '设置主密码(≥4 位)';
      $('#authPassword2').placeholder = '再次确认主密码';
    }
  }

  async function handleAuthSubmit() {
    const pw = $('#authPassword').value;
    const isSetup = !$('#authPassword2').classList.contains('hidden');
    const pw2 = $('#authPassword2').value;
    $('#authError').textContent = '';
    if (!pw) { $('#authError').textContent = '请输入主密码'; return; }
    if (isSetup && pw !== pw2) { $('#authError').textContent = '两次输入不一致'; return; }
    try {
      const path = isSetup ? '/api/setup-password' : '/api/unlock';
      const data = await api('POST', path, { password: pw });
      token = data.token;
      $('#authPassword').value = '';
      $('#authPassword2').value = '';
      showMain();
      await loadItems();
      toast('已解锁');
    } catch (e) {
      $('#authError').textContent = e.message;
    }
  }

  // ---------- 列表 ----------
  async function loadItems() {
    items = await api('GET', '/api/items');
    renderList();
    if (items.length) { selectItem(items[0].id); }
    else { clearEditor(); showEmpty(); }
  }

  function renderList() {
    const q = $('#searchInput').value.trim().toLowerCase();
    const listEl = $('#itemList');
    listEl.innerHTML = '';
    const filtered = items.filter((it) =>
      !q || it.title.toLowerCase().includes(q) || it.category.toLowerCase().includes(q)
    );
    if (!filtered.length) {
      listEl.innerHTML = '<li class="item-empty">' + (q ? '无匹配结果' : '暂无条目,点击"新增"') + '</li>';
      return;
    }
    for (const it of filtered) {
      const li = document.createElement('li');
      li.className = 'item' + (it.id === currentId ? ' active' : '');
      li.dataset.id = it.id;
      li.style.animationDelay = (filtered.indexOf(it) * 24) + 'ms'; // 交错进场
      const cat = it.category ? `<div class="item-cat">${esc(it.category)}</div>` : '';
      li.innerHTML =
        `<div class="item-title">${esc(it.title)}</div>` +
        cat +
        `<div class="item-time">${formatTime(it.updated_at)} · v${it.version_count}</div>`;
      li.addEventListener('click', () => selectItem(it.id));
      listEl.appendChild(li);
    }
  }

  async function selectItem(id) {
    currentId = id;
    renderList();
    try {
      const it = await api('GET', '/api/items/' + id);
      showEditor(it);
      await loadVersions(id);
    } catch (e) {
      toast(e.message, 'err');
    }
  }

  function showEditor(it) {
    $('#emptyHint').classList.add('hidden');
    $('#editor').classList.remove('hidden');
    $('#itemTitle').value = it.title;
    $('#itemCategory').value = it.category || '';
    $('#itemValue').value = it.value || '';
    $('#itemMeta').textContent =
      '创建 ' + formatTime(it.created_at) + ' · 修改 ' + formatTime(it.updated_at);
  }

  function clearEditor() {
    currentId = null;
    $('#emptyHint').classList.remove('hidden');
    $('#editor').classList.add('hidden');
    $('#historyPanel').classList.add('hidden');
    $('#historyList').innerHTML = '';
  }

  function showEmpty() {
    clearEditor();
  }

  // ---------- 新增 / 保存 / 删除 ----------
  async function createNew() {
    // 清空编辑区,进入"新增"模式
    currentId = null;
    renderList();
    $('#emptyHint').classList.add('hidden');
    $('#editor').classList.remove('hidden');
    $('#historyPanel').classList.add('hidden');
    $('#itemTitle').value = '';
    $('#itemCategory').value = '';
    $('#itemValue').value = '';
    $('#itemMeta').textContent = '新条目';
    $('#itemTitle').focus();
  }

  async function save() {
    const title = $('#itemTitle').value.trim();
    const category = $('#itemCategory').value;
    const value = $('#itemValue').value;
    if (!title) { toast('标题不能为空', 'err'); return; }
    try {
      if (currentId === null) {
        const data = await api('POST', '/api/items', { title, category, value });
        currentId = data.id;
        toast('已保存');
      } else {
        await api('PUT', '/api/items/' + currentId, { title, category, value });
        toast('已保存,已记录新版本');
      }
      // 保存后刷新,重新读取最新值与版本
      const it = await api('GET', '/api/items/' + currentId);
      showEditor(it);
      await loadItems();
      await loadVersions(currentId);
    } catch (e) {
      toast(e.message, 'err');
    }
  }

  async function remove() {
    if (currentId === null) { toast('请先选择条目', 'err'); return; }
    if (!confirm('确定删除该条目吗?其所有历史版本也将被删除。')) return;
    try {
      await api('DELETE', '/api/items/' + currentId);
      toast('已删除');
      items = items.filter((i) => i.id !== currentId);
      currentId = null;
      renderList();
      if (items.length) selectItem(items[0].id);
      else showEmpty();
    } catch (e) { toast(e.message, 'err'); }
  }

  // ---------- 版本历史 ----------
  async function loadVersions(id) {
    try {
      const vs = await api('GET', '/api/items/' + id + '/versions');
      const panel = $('#historyPanel');
      const list = $('#historyList');
      list.innerHTML = '';
      if (!vs.length) { panel.classList.add('hidden'); return; }
      panel.classList.remove('hidden');
      for (const v of vs) {
        const row = document.createElement('div');
        row.className = 'history-item';
        row.innerHTML =
          `<div><span class="hver">v${v.version}</span><span class="htime">${formatTime(v.created_at)}</span></div>` +
          `<button class="restore-btn" data-ver="${v.version}">还原此版本</button>`;
        row.querySelector('.restore-btn').addEventListener('click', () => restoreVersion(id, v.version));
        list.appendChild(row);
      }
    } catch (e) { toast(e.message, 'err'); }
  }

  async function restoreVersion(id, version) {
    if (!confirm('确定将当前内容还原为该版本吗?还原会生成一条新的修改记录。')) return;
    try {
      const it = await api('POST', `/api/items/${id}/restore/${version}`);
      showEditor(it);
      await loadItems();
      await loadVersions(id);
      toast('已还原到 v' + version);
    } catch (e) { toast(e.message, 'err'); }
  }

  // ---------- 锁定 ----------
  async function lock() {
    try {
      await api('POST', '/api/lock');
    } catch (_e) { /* 忽略 */ }
    token = null;
    currentId = null;
    items = [];
    configureAuthMode(true);
    showAuth();
    $('#authPassword').focus();
  }

  // ---------- 工具 ----------
  function esc(s) {
    const d = document.createElement('div');
    d.textContent = s == null ? '' : String(s);
    return d.innerHTML;
  }
  function formatTime(iso) {
    if (!iso) return '';
    const d = new Date(iso);
    if (isNaN(d)) return iso;
    const p = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  // ---------- 事件绑定 ----------
  $('#authBtn').addEventListener('click', handleAuthSubmit);
  $('#authPassword').addEventListener('keydown', (e) => { if (e.key === 'Enter') handleAuthSubmit(); });
  $('#authPassword2').addEventListener('keydown', (e) => { if (e.key === 'Enter') handleAuthSubmit(); });
  $('#newBtn').addEventListener('click', createNew);
  $('#saveBtn').addEventListener('click', save);
  $('#deleteBtn').addEventListener('click', remove);
  $('#lockBtn').addEventListener('click', lock);
  $('#dbBtn').addEventListener('click', openDBModal);
  $('#dbCloseBtn').addEventListener('click', () => { $('#dbModal').classList.add('hidden'); });
  $('#exportBtn').addEventListener('click', exportBackup);
  $('#importBtn').addEventListener('click', () => $('#importFile').click());
  $('#importFile').addEventListener('change', (e) => handleImportFile(e.target.files[0]));
  $('#wipeBtn').addEventListener('click', wipe);
  $('#dbModal').addEventListener('click', (e) => { if (e.target === e.currentTarget) $('#dbModal').classList.add('hidden'); });
  $('#searchInput').addEventListener('input', renderList);
  // Ctrl+S 快速保存
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      save();
    }
  });

  initTheme();
  init();
})();