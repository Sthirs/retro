<script setup lang="ts">
/** Three pulsing dots after "… is writing" / "… is editing" (Discuss artboard). */
</script>

<template>
  <span class="dots" aria-hidden="true">
    <span class="dots__dot" />
    <span class="dots__dot" />
    <span class="dots__dot" />
  </span>
</template>

<style scoped>
.dots {
  display: inline-flex;
  gap: 3px;
}
.dots__dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--accent-text);
  animation: dot 1.2s ease-in-out infinite;
}
.dots__dot:nth-child(2) {
  animation-delay: 0.2s;
}
.dots__dot:nth-child(3) {
  animation-delay: 0.4s;
}
@keyframes dot {
  0%,
  100% {
    opacity: 0.3;
  }
  50% {
    opacity: 1;
  }
}
@media (prefers-reduced-motion: reduce) {
  .dots__dot {
    animation: none;
  }
  .dots__dot:nth-child(2) {
    opacity: 0.6;
  }
  .dots__dot:nth-child(3) {
    opacity: 0.3;
  }
}
</style>
