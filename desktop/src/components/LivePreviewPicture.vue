<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from "vue";
import { Play, Volume2, VolumeX, X } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useFastThumbnail } from "@/composables/useFastThumbnail";
import { livePreviews as previews } from "@/composables/useLivePreviews";
const props = defineProps<{ username: string; playable: boolean; thumbnail?: string | null }>();
const { t } = useI18n();
const entry = computed(() => previews.entries.get(props.username));
const image = useFastThumbnail(computed(() => props.thumbnail));
const failed = ref(false);
watch(image, () => { failed.value = false; });
const displayName = computed(() => props.username.replace(/^chaturbate~/, ""));
const muted = computed(() => previews.audible.value !== props.username);
const video = ref<HTMLVideoElement | null>(null);
const playing = ref(false);
const buffering = ref(false);
const src = computed(() => entry.value?.state === "ready" ? entry.value.url : "");
watch(src, () => { playing.value = false; buffering.value = false; });
watch(muted, value => { if (video.value) video.value.muted = value; }, { flush: "sync" });
watch(() => props.playable, playable => { if (!playable && entry.value) void previews.stop(props.username); });
function toggle() {
  if (entry.value && entry.value.state !== "error") void previews.stop(props.username);
  else if (props.playable) void previews.start(props.username);
}
function sound() {
  previews.toggleSound(props.username);
  if (video.value) {
    video.value.muted = muted.value;
    void video.value.play().catch(() => {});
  }
}
function playbackError(event: Event) {
  const element = event.target as HTMLVideoElement;
  const current = entry.value;
  if (current && current.state === "ready" && element === video.value && current.id === element.dataset.previewId) previews.fail(props.username, current.id);
}
onBeforeUnmount(() => { void previews.stop(props.username); });
</script>
<template>
  <div class="relative aspect-video bg-muted overflow-hidden group"
    :class="{ 'cursor-pointer': playable || entry }">
    <video v-if="src" :key="entry!.id" ref="video" :src="src" :data-preview-id="entry!.id"
      :muted="muted" autoplay playsinline preload="auto" class="w-full h-full object-contain bg-black"
      @playing="playing = true; buffering = false" @waiting="buffering = true" @error="playbackError" />
    <img v-else-if="image && !failed" :key="image" :src="image" :alt="displayName" loading="lazy"
      class="w-full h-full object-cover" @error="failed = true" />
    <div v-else class="w-full h-full flex items-center justify-center text-4xl font-bold text-muted-foreground/20">{{ displayName[0]?.toUpperCase() }}</div>
    <div v-if="!entry && playable" class="absolute inset-0 flex items-center justify-center bg-black/10 opacity-80 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none">
      <Play class="size-8 text-white fill-white/20" /><span class="absolute bottom-2 text-xs text-white">{{ t('livePreview.open') }}</span>
    </div>
    <div v-if="entry && (entry.state !== 'ready' || !playing || buffering)" class="absolute inset-0 flex items-center justify-center bg-black/55 text-white text-xs p-4 text-center pointer-events-none" role="status">
      {{ entry.state === 'error' ? (entry.message || t('livePreview.error')) : entry.state === 'retrying' ? t('livePreview.retrying') : buffering || entry.state === 'ready' ? t('livePreview.buffering') : t('livePreview.connecting') }}
    </div>
    <button type="button" class="absolute inset-0 z-10 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary"
      :disabled="!playable && !entry" :aria-label="`${displayName} · ${entry && entry.state !== 'error' ? t('livePreview.close') : t('livePreview.open')}`"
      :aria-pressed="!!entry && entry.state !== 'error'" @click.stop="toggle" />
    <template v-if="entry && entry.state !== 'error'">
      <span class="absolute top-2 left-2 bg-black/60 text-white text-[10px] rounded px-1.5 py-0.5 pointer-events-none">{{ t('livePreview.live') }}</span>
      <div class="absolute right-2 bottom-2 z-20 flex gap-1" @click.stop>
        <button type="button" class="rounded bg-black/65 text-white p-2" :title="muted ? t('livePreview.unmute') : t('livePreview.mute')" :aria-label="muted ? t('livePreview.unmute') : t('livePreview.mute')" :disabled="!src" @click.stop="sound">
          <VolumeX v-if="muted" class="size-4" /><Volume2 v-else class="size-4" />
        </button>
        <button type="button" class="rounded bg-black/65 text-white p-2" :aria-label="t('livePreview.close')" :title="t('livePreview.close')" @click.stop="previews.stop(username)"><X class="size-4" /></button>
      </div>
    </template>
  </div>
</template>
