<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call } from "@/lib/api";
import type { RecordingFile } from "@/types/recordings";
import { recordingName } from "@/lib/recordingGroups";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ChevronLeft, ChevronRight, ExternalLink } from "@lucide/vue";
import { useI18n } from "vue-i18n";
const props=defineProps<{files:RecordingFile[]}>();
const emit=defineEmits<{close:[]}>();
const {t}=useI18n();
const index=ref(0),src=ref(""),error=ref(""),authorizing=ref(false);
const video=ref<HTMLVideoElement>();
const current=computed(()=>props.files[index.value]);
let generation=0;
watch(()=>current.value?.path,async(path)=>{
 const request=++generation;src.value="";error.value="";
 if(!path)return;
 authorizing.value=true;
 try {const authorized=await call<string>('authorize_recording_media',{path});if(request===generation)src.value=convertFileSrc(authorized);}
 catch(e){if(request===generation)error.value=String(e);}
 finally{if(request===generation)authorizing.value=false;}
},{immediate:true});
function ended(){if(index.value<props.files.length-1)index.value++;}
function failed(event:Event){if(event.target===video.value)error.value=t('ui.playbackError');}
async function external(){if(current.value)try{await call('open_recording',{path:current.value.path});}catch(e){error.value=String(e);}}
onBeforeUnmount(()=>{generation++;video.value?.pause();video.value?.removeAttribute('src');video.value?.load();});
</script>
<template>
 <Dialog :open="true" @update:open="v=>!v&&emit('close')"><DialogContent class="sm:max-w-[900px] w-[90vw]"><DialogHeader><DialogTitle>{{ t('ui.playback') }} · {{ index+1 }} / {{ files.length }}</DialogTitle><DialogDescription class="break-all">{{ current?.name }}</DialogDescription></DialogHeader>
  <div class="rounded-lg overflow-hidden bg-black min-h-48 flex items-center justify-center">
   <video v-if="src && !error" :key="current?.path" ref="video" :src="src" controls autoplay playsinline preload="metadata" class="w-full max-h-[58vh] object-contain" @ended="ended" @error="failed" />
   <p v-else class="text-xs text-white/80 p-8" role="status">{{ authorizing ? t('common.loading') : error || t('ui.noData') }}</p>
  </div>
  <div class="flex items-center justify-between gap-2"><Button variant="outline" size="sm" :disabled="index===0" @click="index--"><ChevronLeft class="size-4" />{{ t('ui.previousPart') }}</Button><Button variant="ghost" size="sm" @click="external"><ExternalLink class="size-4" />{{ t('ui.external') }}</Button><Button variant="outline" size="sm" :disabled="index===files.length-1" @click="index++">{{ t('ui.nextPart') }}<ChevronRight class="size-4" /></Button></div>
  <div class="flex flex-wrap gap-1.5 max-h-20 overflow-auto" :aria-label="t('ui.sequential')"><button v-for="(file,i) in files" :key="file.path" class="queue-part" :class="{active:index===i}" :aria-current="index===i?'true':undefined" @click="index=i">{{ recordingName(file).part != null ? `part${String(recordingName(file).part).padStart(2,'0')}` : String(i+1) }}</button></div>
 </DialogContent></Dialog>
</template>
