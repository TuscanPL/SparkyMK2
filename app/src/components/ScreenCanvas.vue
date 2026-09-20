<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { HEIGHT, WIDTH, draw, type Pixels } from "../screens";

const props = defineProps<{ pixels: Pixels | null; scale: number }>();
const canvas = ref<HTMLCanvasElement>();

function paint() {
  const el = canvas.value;
  if (!el) return;
  if (props.pixels) {
    draw(el, props.pixels);
  } else {
    el.getContext("2d")?.clearRect(0, 0, WIDTH, HEIGHT);
  }
}

onMounted(paint);
watch(() => props.pixels, paint);
</script>

<template>
  <canvas
    ref="canvas"
    :width="WIDTH"
    :height="HEIGHT"
    :style="{ width: `${WIDTH * scale}px`, height: `${HEIGHT * scale}px` }"
  />
</template>

<style scoped>
canvas {
  display: block;
  image-rendering: pixelated;
  background: #1a140a;
  border-radius: 3px;
}
</style>
