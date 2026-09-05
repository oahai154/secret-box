<script lang="ts">
  // 通用确认弹窗（复用 Go 版 card-verify 样式）
  let {
    title,
    message,
    confirmText = "确认",
    onConfirm,
    onCancel,
  }: {
    title: string;
    message: string;
    confirmText?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-mask" onclick={(e) => {
  if (e.target === e.currentTarget) onCancel();
}}>
  <div class="modal card-verify">
    <h2 class="modal-title">{title}</h2>
    <p class="verify-hint">{message}</p>
    <div class="modal-actions">
      <button class="btn btn-ghost" type="button" onclick={onCancel}>取消</button>
      <button class="btn btn-danger" type="button" onclick={onConfirm}>{confirmText}</button>
    </div>
  </div>
</div>
