<script setup lang="ts">
defineProps<{ label: string }>();
const model = defineModel<boolean>({ required: true });
</script>

<template>
  <label class="checkbox">
    <input v-model="model" type="checkbox" class="checkbox__input" />
    {{ label }}
  </label>
</template>

<style scoped>
.checkbox {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
  color: var(--text);
  cursor: pointer;
}
.checkbox__input {
  width: 18px;
  height: 18px;
  margin: 0;
  accent-color: var(--accent);
}
</style>
