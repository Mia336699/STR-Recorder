<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { useStreamersStore } from "@/stores/streamers";
import { useNotify } from "@/composables/useNotify";
import { call } from "@/lib/api";
import { roomIdentity, roomDisplayName, roomPlatform, type RoomPlatform } from "@/lib/rooms";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "vue-i18n";
const emit = defineEmits<{close:[];added:[]}>();
const store = useStreamersStore();
const { toast } = useNotify(); const { t } = useI18n();
const username = ref(""); const platform = ref<RoomPlatform>("stripchat"); const loading = ref(false); const auto = ref(false);
const mirror = ref<string|null>(null); const globalAuto = ref(true);
const identity = computed(() => roomIdentity(username.value,platform.value,mirror.value));
const duplicate = computed(() => !!identity.value && store.streamers.some(s => s.username === identity.value));
const validation = computed(() => !username.value.trim() ? "" : !identity.value ? t('ui.invalidRoom') : duplicate.value ? t('ui.duplicateRoom') : "");
const check = ref<{state:'idle'|'checking'|'ready'|'missing'|'error';status?:string}>({state:'idle'});
let timer:ReturnType<typeof setTimeout>|undefined;let generation=0;
onMounted(async () => {try { const s=await call<{sc_mirror_url:string|null;auto_record:boolean}>('get_settings');mirror.value=s.sc_mirror_url;globalAuto.value=s.auto_record; } catch {} });
watch([identity,duplicate],([name,exists]) => {
 clearTimeout(timer);const current=++generation;check.value={state:'idle'};
 if(!name||exists)return;
 timer=setTimeout(async()=>{check.value={state:'checking'};try {const r=await call<{exists:boolean;status?:string}>('verify_streamer',{username:name});if(current===generation)check.value={state:r.exists?'ready':'missing',status:r.status};}catch{if(current===generation)check.value={state:'error'};}},400);
});
onBeforeUnmount(()=>{clearTimeout(timer);generation++;});
async function submit(){const name=identity.value;if(!name||validation.value||loading.value||check.value.state==='checking'||check.value.state==='missing')return;loading.value=true;
 try {await store.addStreamer(name,auto.value && globalAuto.value);toast(t('addStreamer.done',{name:roomDisplayName(name)}),'success');emit('added');}catch(e){toast(String(e),'error');}finally{loading.value=false;}
}
</script>
<template>
 <Dialog :open="true" @update:open="v => !v && emit('close')"><DialogContent class="sm:max-w-[488px]"><DialogHeader><DialogTitle>{{ t('addStreamer.title') }}</DialogTitle><DialogDescription>{{ t('ui.addHint') }}</DialogDescription></DialogHeader>
 <form @submit.prevent="submit" class="flex flex-col gap-4 py-2">
  <div v-if="!username.includes('://') && !username.startsWith('chaturbate~')" class="flex items-center gap-3"><Label for="platform">{{ t('addStreamer.platform') }}</Label><select id="platform" v-model="platform" :disabled="loading"><option value="stripchat">Stripchat</option><option value="chaturbate">Chaturbate</option></select></div>
  <div class="space-y-2"><Label for="username">{{ t('addStreamer.label') }}</Label><Input id="username" v-model="username" :placeholder="t('addStreamer.placeholder')" autofocus :disabled="loading" :aria-invalid="!!validation" :aria-describedby="validation ? 'room-validation' : undefined" />
   <p v-if="validation" id="room-validation" class="text-xs text-destructive" role="alert">{{ validation }}</p>
  </div>
  <div v-if="identity && !validation" class="rounded-lg border bg-muted/40 p-3 space-y-1" aria-live="polite"><strong class="text-sm">{{ roomDisplayName(identity) }}</strong><p class="text-xs text-muted-foreground">{{ roomPlatform(identity) }} · {{ check.state === 'checking' ? t('common.loading') : check.state === 'missing' ? t('finder.card.notFound') : check.state === 'error' ? t('finder.card.timeout') : check.status || t('ui.identityHint',{platform:roomPlatform(identity),name:roomDisplayName(identity)}) }}</p></div>
  <div class="flex items-start gap-3"><Switch id="add-auto" v-model="auto" :disabled="!globalAuto || loading" /><div><Label for="add-auto">{{ t('ui.auto') }}</Label><p class="text-xs text-muted-foreground mt-1">{{ globalAuto ? t('ui.autoAddHint') : t('ui.globalAutoDisabled') }}</p></div></div>
  <DialogFooter><Button type="button" variant="outline" :disabled="loading" @click="emit('close')">{{ t('addStreamer.cancel') }}</Button><Button type="submit" :disabled="loading || !identity || !!validation || check.state === 'checking' || check.state === 'missing'">{{ loading ? t('addStreamer.submitting') : t('addStreamer.submit') }}</Button></DialogFooter>
 </form></DialogContent></Dialog>
</template>
