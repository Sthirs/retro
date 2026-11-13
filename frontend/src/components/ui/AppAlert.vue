<script setup lang="ts">
/** Inline error message, announced to screen readers. */
</script>

<template>
  <div class="alert" role="alert">
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="10" />
      <path d="M12 8v4M12 16h.01" />
    </svg>
    <span><slot /></span>
  </div>
</template>

<style scoped>
.alert {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 10px 14px;
  border-radius: var(--radius-control);
  border: 1px solid var(--danger);
  color: var(--danger);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.4;
  text-align: left;
}
.alert svg {
  flex: none;
}
</style>
