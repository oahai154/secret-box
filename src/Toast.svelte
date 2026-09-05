<script lang="ts">
  let { message = "", type = "" }: { message?: string; type?: string } = $props();
</script>

<div id="toast" class="toast {type}" class:show={!!message}>{message}</div>
