<script setup lang="ts">
defineProps<{ name: keyof typeof paths; size?: number }>();
</script>

<script lang="ts">
const paths = {
  refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
  play: "M7 4.5v15l12-7.5-12-7.5z",
  lock: "M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z",
  plug: "M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0V8zM12 18v4",
  close: "M6 6l12 12M18 6 6 18",
  usb: "M12 2v14M8 6l4-4 4 4M7 11v2l5 3 5-3v-3M7 11h0M17 10h0M12 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  up: "M12 19V5M5 12l7-7 7 7",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z",
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5zM14 3v5h5",
  stop: "M6 6h12v12H6z",
  chevron: "M9 6l6 6-6 6",
};
</script>

<template>
  <svg
    :width="size ?? 16"
    :height="size ?? 16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="paths[name]" />
  </svg>
</template>
