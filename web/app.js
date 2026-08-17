/* SecretBox 前端逻辑 */
(function () {
  'use strict';

  const $ = (sel) => document.querySelector(sel);
  let token = null;
  let currentId = null;
  let items = [];
  let valueVisible = false;

  // ---------- Settings ----------
  let settings = {
    auto_lock_seconds: '120',
    delete_requires_password: 'true',
    delete_version_requires_password: 'true',
  };
  let autoLockTimer = null;
  let pendingDeleteAction = null; // { type: 'item'|'version', id, version? }

  async function loadSettings() {
    try {
      const s = await api('GET', '/api/settings');
      Object.assign(settings, s);
    } catch (_e) { /* 使用默认值 */ }
  }

  async function saveSettings(updates) {
    Object.assign(settings, updates);
    try {
      await api('PUT', '/api/settings', updates);
    } catch (e) {
      toast('保存设置失败: ' + e.message, 'err');
    }
    resetAutoLockTimer();
  }

  // ---------- Auto Lock Timer ----------
  function resetAutoLockTimer() {
    clearTimeout(autoLockTimer);
    const secs = parseInt(settings.auto_lock_seconds, 10);
    if (secs > 0) {
      autoLockTimer = setTimeout(() => {
        toast('长时间无操作,已自动锁定');
        lock();
      }, secs * 1000);
    }
  }

  function resetActivityTimer() {
    if (token) resetAutoLockTimer();
  }

  // ---------- Password Verify Modal ----------
  function openVerifyModal(action) {
    pendingDeleteAction = action;
    $('#verifyPasswordInput').value = '';
    $('#verifyError').textContent = '';
    if (action.type === 'item') {
      $('#verifyHint').textContent = '删除条目需要验证主密码';
    } else {
      $('#verifyHint').textContent = '删除历史版本需要验证主密码';
    }
    $('#verifyPasswordModal').classList.remove('hidden');
    setTimeout(() => $('#verifyPasswordInput').focus(), 100);
  }

  async function confirmVerify() {
    const pw = $('#verifyPasswordInput').value;
    if (!pw) { $('#verifyError').textContent = '请输入密码'; return; }
    try {
      // 验证密码:尝试解锁
      const data = await api('POST', '/api/unlock', { password: pw });
      token = data.token;
      resetAutoLockTimer();
      $('#verifyPasswordModal').classList.add('hidden');
      // 执行待处理的删除操作
      if (pendingDeleteAction) {
        if (pendingDeleteAction.type === 'item') {
          await doDeleteItem();
        } else {
          await doDeleteVersion(pendingDeleteAction.id, pendingDeleteAction.version);
        }
        pendingDeleteAction = null;
      }
    } catch (e) {
      $('#verifyError').textContent = '密码不正确';
    }
  }

  // ---------- Change Password ----------
  function openChangePasswordModal() {
    $('#oldPasswordInput').value = '';
    $('#newPasswordInput').value = '';
    $('#confirmPasswordInput').value = '';
    $('#passwordError').textContent = '';
    $('#settingsModal').classList.add('hidden');
    $('#changePasswordModal').classList.remove('hidden');
    setTimeout(() => $('#oldPasswordInput').focus(), 100);
  }

  async function confirmChangePassword() {
    const oldPw = $('#oldPasswordInput').value;
    const newPw = $('#newPasswordInput').value;
    const confirmPw = $('#confirmPasswordInput').value;
    $('#passwordError').textContent = '';

    if (!oldPw) { $('#passwordError').textContent = '请输入当前密码'; return; }
    if (newPw.length < 4) { $('#passwordError').textContent = '新密码至少 4 位'; return; }
    if (newPw !== confirmPw) { $('#passwordError').textContent = '两次输入的新密码不一致'; return; }

    try {
      await api('POST', '/api/change-password', { old_password: oldPw, new_password: newPw });
      toast('密码修改成功', 'ok');
      $('#changePasswordModal').classList.add('hidden');
    } catch (e) {
      $('#passwordError').textContent = e.message;
    }
  }

  // ---------- Settings Modal ----------
  function openSettingsModal() {
    $('#autoLockSelect').value = settings.auto_lock_seconds;
    $('#deleteRequiresPassword').checked = settings.delete_requires_password === 'true';
    $('#deleteVersionRequiresPassword').checked = settings.delete_version_requires_password === 'true';
    $('#settingsModal').classList.remove('hidden');
  }

  function closeSettingsModal() {
    $('#settingsModal').classList.add('hidden');
  }

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

  // ---------- 自定义分类选择器 ----------
  const categorySelect = $('#categorySelect');
  const categoryTrigger = categorySelect.querySelector('.custom-select-trigger');
  const categoryValueEl = categorySelect.querySelector('.custom-select-value');
  const categoryDropdown = categorySelect.querySelector('.custom-select-dropdown');
  let selectedCategory = '';

  function setCategory(val) {
    selectedCategory = val;
    const li = categoryDropdown.querySelector(`li[data-value="${CSS.escape(val)}"]`);
    categoryDropdown.querySelectorAll('li').forEach((el) => el.classList.remove('selected'));
    if (li) {
      li.classList.add('selected');
      categoryValueEl.textContent = li.textContent;
    } else {
      categoryValueEl.textContent = '通用';
    }
  }

  categoryTrigger.addEventListener('click', () => {
    categorySelect.classList.toggle('open');
  });

  categoryDropdown.addEventListener('click', (e) => {
    const li = e.target.closest('li');
    if (li) {
      setCategory(li.dataset.value);
      categorySelect.classList.remove('open');
    }
  });

  // 点击外部关闭
  document.addEventListener('click', (e) => {
    if (!categorySelect.contains(e.target)) {
      categorySelect.classList.remove('open');
    }
  });

  // ---------- 保密内容显示/隐藏 ----------
  const toggleValueBtn = $('#toggleValueBtn');
  const itemValueEl = $('#itemValue');
  const toggleText = toggleValueBtn.querySelector('.toggle-text');

  toggleValueBtn.addEventListener('click', () => {
    valueVisible = !valueVisible;
    if (valueVisible) {
      itemValueEl.classList.remove('value-hidden');
      itemValueEl.classList.add('value-visible');
      toggleText.textContent = '点击隐藏';
    } else {
      itemValueEl.classList.remove('value-visible');
      itemValueEl.classList.add('value-hidden');
      toggleText.textContent = '点击显示';
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
          await loadSettings();
          await loadItems();
          resetAutoLockTimer();
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
      await loadSettings();
      await loadItems();
      resetAutoLockTimer();
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
    setCategory(it.category || '');
    $('#itemNote').value = it.note || '';
    $('#itemValue').value = it.value || '';
    // 每次切换条目时,默认隐藏保密内容
    valueVisible = false;
    itemValueEl.classList.remove('value-visible');
    itemValueEl.classList.add('value-hidden');
    toggleText.textContent = '点击显示';
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
    setCategory('');
    $('#itemNote').value = '';
    $('#itemValue').value = '';
    // 默认隐藏保密内容
    valueVisible = false;
    itemValueEl.classList.remove('value-visible');
    itemValueEl.classList.add('value-hidden');
    toggleText.textContent = '点击显示';
    $('#itemMeta').textContent = '新条目';
    $('#itemTitle').focus();
  }

  async function save() {
    const title = $('#itemTitle').value.trim();
    const category = selectedCategory;
    const note = $('#itemNote').value;
    const value = $('#itemValue').value;
    if (!title) { toast('标题不能为空', 'err'); return; }
    try {
      if (currentId === null) {
        const data = await api('POST', '/api/items', { title, category, note, value });
        currentId = data.id;
        toast('已保存');
      } else {
        await api('PUT', '/api/items/' + currentId, { title, category, note, value });
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
    if (settings.delete_requires_password === 'true') {
      openVerifyModal({ type: 'item', id: currentId });
    } else {
      await doDeleteItem();
    }
  }

  async function doDeleteItem() {
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
          `<div class="history-actions">` +
          `<button class="restore-btn" data-ver="${v.version}">还原</button>` +
          `<button class="delete-ver-btn" data-ver="${v.version}" title="删除此版本">✕</button>` +
          `</div>`;
        row.querySelector('.restore-btn').addEventListener('click', () => restoreVersion(id, v.version));
        row.querySelector('.delete-ver-btn').addEventListener('click', () => {
          if (settings.delete_version_requires_password === 'true') {
            openVerifyModal({ type: 'version', id: id, version: v.version });
          } else {
            doDeleteVersion(id, v.version);
          }
        });
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

  async function doDeleteVersion(id, version) {
    try {
      await api('DELETE', `/api/items/${id}/versions/${version}`);
      toast('已删除 v' + version);
      await loadVersions(id);
    } catch (e) { toast(e.message, 'err'); }
  }

  // ---------- 锁定 ----------
  async function lock() {
    clearTimeout(autoLockTimer);
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

  // Settings
  $('#settingsBtn').addEventListener('click', openSettingsModal);
  $('#settingsCloseBtn').addEventListener('click', closeSettingsModal);
  $('#settingsModal').addEventListener('click', (e) => { if (e.target === e.currentTarget) closeSettingsModal(); });
  $('#autoLockSelect').addEventListener('change', (e) => {
    saveSettings({ auto_lock_seconds: e.target.value });
  });
  $('#deleteRequiresPassword').addEventListener('change', (e) => {
    saveSettings({ delete_requires_password: e.target.checked ? 'true' : 'false' });
  });
  $('#deleteVersionRequiresPassword').addEventListener('change', (e) => {
    saveSettings({ delete_version_requires_password: e.target.checked ? 'true' : 'false' });
  });
  $('#changePasswordBtn').addEventListener('click', openChangePasswordModal);
  $('#cancelPasswordBtn').addEventListener('click', () => {
    $('#changePasswordModal').classList.add('hidden');
    openSettingsModal();
  });
  $('#confirmPasswordBtn').addEventListener('click', confirmChangePassword);
  $('#changePasswordModal').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) {
      $('#changePasswordModal').classList.add('hidden');
      openSettingsModal();
    }
  });

  // Verify password modal
  $('#cancelVerifyBtn').addEventListener('click', () => {
    $('#verifyPasswordModal').classList.add('hidden');
    pendingDeleteAction = null;
  });
  $('#confirmVerifyBtn').addEventListener('click', confirmVerify);
  $('#verifyPasswordInput').addEventListener('keydown', (e) => { if (e.key === 'Enter') confirmVerify(); });
  $('#verifyPasswordModal').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) {
      $('#verifyPasswordModal').classList.add('hidden');
      pendingDeleteAction = null;
    }
  });

  // Ctrl+S 快速保存
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      save();
    }
  });

  // Activity timer reset on user interactions
  ['click', 'keydown', 'scroll', 'mousemove'].forEach((evt) => {
    document.addEventListener(evt, resetActivityTimer, { passive: true });
  });

  initTheme();
  init();
})();