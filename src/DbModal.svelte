<script lang="ts">
  // 数据备份与迁移弹窗（复刻 Go 版 dbModal）
  import type { Item } from "./ipc";

  let {
    items,
    hasPassword,
    onClose,
    onExport,
    onExportCsv,
    onImportFile,
    onWipe,
  }: {
    items: Item[];
    hasPassword: boolean;
    onClose: () => void;
    onExport: () => void;
    onExportCsv: () => void;
    onImportFile: (file: File) => void;
    onWipe: () => void;
  } = $props();

  let fileInput: HTMLInputElement | undefined = $state();

  function handleFileChange(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) onImportFile(file);
    // 允许重复选择同一文件
    input.value = "";
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-mask" onclick={(e) => {
  if (e.target === e.currentTarget) onClose();
}}>
  <div class="modal card-db">
    <h2 class="modal-title">数据备份与迁移</h2>

    <div class="db-section-title">当前数据库</div>
    <div id="dbInfo" class="db-info">
      <div class="db-row"><span class="db-label">状态</span><span class="db-value">✅ 正常</span></div>
      <div class="db-row"><span class="db-label">条目</span><span class="db-value">{items.length} 条</span></div>
      <div class="db-row"><span class="db-label">主密码</span><span class="db-value">{hasPassword ? "✅ 已设置" : "未设置"}</span></div>
    </div>

    <div class="db-actions">
      <button id="exportBtn" class="btn btn-primary btn-block" type="button" onclick={onExport}>导出迁移文件(备份)</button>
      <div class="export-hint">用一个独立的迁移口令加密整个库(含历史版本),保存后可用 "<b>导入迁移文件</b>" 在新位置还原。</div>

      <button id="exportCsvBtn" class="btn btn-ghost btn-block" type="button" onclick={onExportCsv}>导出明文 CSV</button>
      <div class="export-hint">不加密导出全部条目(仅当前版本,不含历史版本),供迁往其他密码管理器或表格软件查看。<b>任何拿到此文件的人都能读取全部内容</b>,请妥善保管、用完即删;完整备份请用"导出迁移文件"。</div>

      <button id="importBtn" class="btn btn-ghost btn-block" type="button" onclick={() => fileInput?.click()}>导入迁移文件(还原)</button>
      <input type="file" id="importFile" accept=".secretbox" class="hidden" bind:this={fileInput} onchange={handleFileChange} />
      <div class="export-hint">选择之前导出的迁移文件并输入该迁移口令,将完全还原其中的数据。</div>

      <button id="wipeBtn" class="btn btn-danger btn-block" type="button" onclick={onWipe}>清除本地全部数据</button>
      <div class="export-hint">彻底删除本地所有条目、历史与主密码设置。<b>必须先完成迁移文件导出并输入主密码验证后才能清除</b>,以防误操作造成数据丢失。</div>
    </div>

    <div class="modal-actions">
      <button id="dbCloseBtn" class="btn btn-ghost" type="button" onclick={onClose}>关闭</button>
    </div>
  </div>
</div>
