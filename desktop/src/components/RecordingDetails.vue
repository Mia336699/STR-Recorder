<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { call } from "@/lib/api";
import type { RecordingFile } from "@/types/recordings";
import type { RecordingGroup } from "@/lib/recordingGroups";
import { recordingName } from "@/lib/recordingGroups";
import { formatSize, formatDuration } from "@/utils/format";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Play, Image, FolderOpen } from "@lucide/vue";
const props=defineProps<{file:RecordingFile;group:RecordingGroup;stats?:{downloaded:number;failed:number};speed?:number;outputs?:Record<string,string>;processing?:string}>();
const emit=defineEmits<{close:[];play:[file:RecordingFile];preview:[]}>();
const {t,locale}=useI18n();
const tab=ref('media');
const media=ref<{codec_name?:string;codec_type?:string;width?:number;height?:number}[]>([]);
const probeError=ref('');let generation=0;
const finalized=computed(()=>!props.file.is_recording&&!props.file.status?.startsWith('merging'));
watch(()=>[props.file.path,finalized.value],async()=>{const request=++generation;media.value=[];probeError.value='';if(!finalized.value)return;try{const r=await call<{streams:typeof media.value}>('inspect_recording',{path:props.file.path});if(request===generation)media.value=r.streams||[];}catch(e){if(request===generation)probeError.value=String(e);}}, {immediate:true});
onBeforeUnmount(()=>generation++);
const videoStream=computed(()=>media.value.find(s=>s.codec_type==='video'));
const audioStream=computed(()=>media.value.find(s=>s.codec_type==='audio'));
const part=computed(()=>recordingName(props.file).part);
const gap=computed(()=>{const index=props.group.files.findIndex(f=>f.path===props.file.path);if(index<1)return null;const previous=props.group.files[index-1];const duration=previous.video_duration_secs??previous.record_duration_secs;if(duration==null)return null;const seconds=Math.round((Date.parse(props.file.started_at)-Date.parse(previous.started_at))/1000-duration);return seconds>1?seconds:null;});
function date(value:string){const d=new Date(value);return Number.isFinite(d.getTime())?new Intl.DateTimeFormat(locale.value,{year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit',hour12:false}).format(d):t('ui.noData');}
async function locate(){try{await call('reveal_recording',{path:props.file.path});}catch(e){probeError.value=String(e);}}
</script>
<template>
 <Dialog :open="true" @update:open="v=>!v&&emit('close')"><DialogContent class="recording-drawer"><DialogHeader><DialogTitle>{{ t('ui.details') }}<span v-if="part != null"> · part{{ String(part).padStart(2,'0') }}</span></DialogTitle><DialogDescription class="break-all pt-2">{{ file.name }}</DialogDescription></DialogHeader>
  <div class="filter-tabs my-2" role="tablist" :aria-label="t('ui.details')"><button v-for="key in ['media','stats','post']" :key="key" class="filter-tab" role="tab" :aria-selected="tab===key" :class="{active:tab===key}" @click="tab=key">{{ t(key==='media'?'ui.mediaTime':key==='stats'?'ui.statistics':'nav.postprocess') }}</button></div>
  <template v-if="tab==='media'"><dl class="recording-facts"><dt>{{ t('ui.platform') }} / {{ t('ui.streamer') }}</dt><dd>{{ group.platform }} / {{ group.username }}</dd><dt>{{ t('ui.groupStart') }}</dt><dd>{{ date(group.started_at) }}</dd><dt>{{ t('ui.actualStart') }}</dt><dd>{{ date(file.started_at) }}</dd><dt>{{ t('recordings.table.videoDuration') }}</dt><dd>{{ file.video_duration_secs != null ? formatDuration(file.video_duration_secs) : t('ui.noData') }}</dd><dt>{{ t('recordings.table.size') }}</dt><dd>{{ formatSize(file.size_bytes) }}</dd><dt>{{ t('ui.mediaVideo') }}</dt><dd>{{ videoStream?.codec_name || t('ui.noData') }}<span v-if="videoStream?.width"> · {{ videoStream.width }} × {{ videoStream.height }}</span></dd><dt>{{ t('ui.mediaAudio') }}</dt><dd>{{ audioStream?.codec_name || t('ui.noData') }}</dd></dl><p v-if="probeError" class="text-xs text-muted-foreground break-words">{{ probeError }}</p>
   <div class="flex flex-wrap gap-1.5 mt-5"><span v-for="f in group.files" :key="f.path" class="queue-part" :class="{active:f.path===file.path}">{{ recordingName(f).part != null ? `part${String(recordingName(f).part).padStart(2,'0')}` : '—' }}</span></div><p v-if="gap" class="detail-warning mt-3">{{ t('ui.gapHint',{seconds:gap}) }}</p>
  </template>
  <dl v-else-if="tab==='stats'" class="recording-facts"><dt>{{ t('ui.savedSegments') }}</dt><dd>{{ stats?.downloaded ?? file.segments_downloaded ?? t('ui.noData') }}</dd><dt>{{ t('ui.failedSegments') }}</dt><dd>{{ stats?.failed ?? file.segments_failed ?? t('ui.noData') }}</dd><dt>{{ t('recordings.table.recordDuration') }}</dt><dd>{{ file.record_duration_secs != null ? formatDuration(file.record_duration_secs) : t('ui.noData') }}</dd><dt>{{ t('recordings.table.speed') }}</dt><dd>{{ file.is_recording && speed != null ? `${formatSize(speed)}/s` : t('ui.noData') }}</dd></dl>
  <div v-else class="space-y-3"><p class="text-xs text-muted-foreground">{{ processing || file.status || t('ui.noData') }}</p><div v-for="(result,index) in file.pp_results || []" :key="index" class="border rounded-lg p-3"><strong class="text-sm">{{ result.module_id }}</strong><p class="text-xs mt-1" :class="result.success?'text-[var(--live)]':'text-destructive'">{{ result.message }}</p></div><Button v-if="outputs?.contact_sheet" variant="outline" @click="emit('preview')"><Image class="size-4" />{{ t('ui.previewImage') }}</Button><p v-else-if="!file.pp_results?.length" class="text-xs text-muted-foreground">{{ t('ui.noData') }}</p></div>
  <div class="flex gap-2 mt-auto pt-5"><Button :disabled="!finalized" @click="emit('play',file)"><Play class="size-4" />{{ t('recordings.actions.play') }}</Button><Button variant="outline" :disabled="!finalized" @click="locate"><FolderOpen class="size-4" />{{ t('ui.reveal') }}</Button></div>
 </DialogContent></Dialog>
</template>
