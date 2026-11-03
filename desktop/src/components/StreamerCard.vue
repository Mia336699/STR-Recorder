<script setup lang="ts">
import { computed } from "vue";
import type { StreamerEntry } from "@/stores/streamers";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { useI18n } from "vue-i18n";
import { roomDisplayName, roomPlatform } from "@/lib/rooms";
import { livePreviews } from "@/composables/useLivePreviews";
import LivePreviewPicture from "./LivePreviewPicture.vue";
import ActionMenu from "./ActionMenu.vue";
const props = defineProps<{streamer:StreamerEntry}>();
const emit = defineEmits<{remove:[]; start:[]; stop:[]; 'toggle-auto':[enabled:boolean]}>();
const { t } = useI18n();
const displayName = computed(() => roomDisplayName(props.streamer.username));
const platform = computed(() => roomPlatform(props.streamer.username));
const watching = computed(() => { const e = livePreviews.entries.get(props.streamer.username); return !!e && e.state !== 'error'; });
</script>
<template>
  <article class="streamer-card" :class="{recording:streamer.is_recording,watching,offline:!streamer.is_online && !!streamer.status}" :aria-label="displayName">
    <div class="streamer-media">
      <LivePreviewPicture :username="streamer.username" :playable="streamer.is_online && streamer.is_recordable" :thumbnail="streamer.thumbnail_url" />
      <span v-if="!watching" class="picture-state" :class="{public:streamer.is_recordable,restricted:streamer.is_online && !streamer.is_recordable}">{{ streamer.is_online ? streamer.status : streamer.status ? t('streamerCard.offline') : t('common.loading') }}</span>
      <span v-if="!streamer.is_online && streamer.status" class="stale-note">{{ t('ui.stale') }}</span>
    </div>
    <div class="streamer-body"><div class="streamer-identity">
      <div class="streamer-name"><strong :title="displayName">{{ displayName }}</strong><ActionMenu :label="`${displayName} · ${t('ui.more')}`" :items="[{key:'remove',label:t('streamerCard.removeTitle'),danger:true}]" @select="emit('remove')" /></div>
      <div class="streamer-meta"><span class="platform-mark" :class="{cb:platform === 'Chaturbate'}">{{ platform[0] }}</span><span>{{ platform }}</span><span class="streamer-state" :class="{rec:streamer.is_recording}">{{ streamer.is_recording ? `● ${t('streamerCard.recording')}` : streamer.is_online ? t('ui.online') : streamer.status ? t('streamerCard.offline') : t('common.loading') }}</span></div>
    </div><div class="streamer-actions">
      <button type="button" :class="{stop:streamer.is_recording}" :disabled="!streamer.is_recording && !streamer.is_recordable" :title="!streamer.is_recordable ? streamer.status : ''" @click="streamer.is_recording ? emit('stop') : emit('start')">{{ streamer.is_recording ? `■ ${t('streamerCard.stopRecording')}` : `● ${t('streamerCard.startRecording')}` }}</button>
      <div class="streamer-auto"><Switch :id="`auto-${streamer.username}`" :aria-label="`${displayName} · ${t('ui.auto')}`" :model-value="streamer.auto_record" @update:model-value="emit('toggle-auto',$event)" /><Label :for="`auto-${streamer.username}`">{{ t('ui.auto') }}</Label></div>
    </div></div>
  </article>
</template>
