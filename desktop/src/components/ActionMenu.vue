<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { MoreHorizontal } from "@lucide/vue";
import { useI18n } from "vue-i18n";
defineProps<{ label?: string; items: { key: string; label: string; danger?: boolean; disabled?: boolean }[] }>();
const emit = defineEmits<{ select: [key: string] }>();
const { t } = useI18n();
const root = ref<HTMLDetailsElement>();
function close(restore = false) { if (root.value) { root.value.open = false; if (restore) root.value.querySelector("summary")?.focus(); } }
function outside(event: Event) { if (event.target instanceof Node && !root.value?.contains(event.target)) close(); }
function select(key: string) { close(true); emit("select", key); }
onMounted(() => { document.addEventListener("pointerdown", outside); document.addEventListener("focusin", outside); });
onBeforeUnmount(() => { document.removeEventListener("pointerdown", outside); document.removeEventListener("focusin", outside); });
</script>
<template>
  <details ref="root" class="card-menu" @keydown.esc.stop.prevent="close(true)">
    <summary :aria-label="label || t('ui.more')" :title="label || t('ui.more')"><MoreHorizontal /></summary>
    <div><button v-for="item in items" :key="item.key" type="button" :disabled="item.disabled" :class="{'text-destructive':item.danger}" @click="select(item.key)">{{ item.label }}</button></div>
  </details>
</template>
