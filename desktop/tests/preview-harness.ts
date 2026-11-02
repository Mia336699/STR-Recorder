// Native test harness: production cards/store/IPC, synthetic streams, isolated temporary state.
import { createApp, ref, h, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { createPinia } from "pinia";
import StreamerCard from "../src/components/StreamerCard.vue";
import i18n from "../src/i18n";
import { livePreviews } from "../src/composables/useLivePreviews";
import "../src/style.css";
const initialRooms = ["qa_one", "chaturbate~qa_two", "qa_three", "chaturbate~qa_four"];
createApp({setup() {
  const visible=ref(true);
  const rooms=ref([...initialRooms]);
  const realRoom=ref("");
  const metrics=ref("等待开启预览");
  const timer=setInterval(async()=>{
    const videos=[...document.querySelectorAll("video")];
    const active=await invoke<number>("qa_preview_count");
    metrics.value=`后端会话 ${active} · 播放器 ${videos.length} · 播放中 ${videos.filter(v=>v.currentTime>0&&!v.paused).length} · 有声 ${videos.filter(v=>!v.muted).length} · 时间 ${videos.map(v=>v.currentTime.toFixed(1)).join(" / ")} · 解码帧 ${videos.map(v=>v.getVideoPlaybackQuality?.().totalVideoFrames ?? 0).join(" / ")}`;
  },500);
  onBeforeUnmount(()=>clearInterval(timer));
  onBeforeUnmount(()=>{void livePreviews.stopAll();});
  return ()=>h("div",{class:"dark min-h-screen bg-background text-foreground p-6",style:{width:"1000px",transform:"scale(0.8)",transformOrigin:"top left"}},[
    h("p",{role:"status",class:"text-sm mb-2"},metrics.value),
    h("h1",{class:"text-xl font-bold mb-3"},rooms.value.some(r=>!initialRooms.includes(r))?"真实公开房间预览验收":"四路直播预览验收 · 本地合成画面"),
    h("button",{class:"border rounded px-3 py-2 mb-3",onClick:()=>{visible.value=!visible.value;if(!visible.value)void livePreviews.stopAll();}},visible.value?"离开主播列表":"返回主播列表"),
    h("div",{class:"flex gap-2 mb-3"},[
      h("input",{class:"border rounded px-2 py-1 bg-background w-80",placeholder:"公开测试房间链接",value:realRoom.value,onInput:(event:Event)=>{realRoom.value=(event.target as HTMLInputElement).value;}}),
      h("button",{class:"border rounded px-2",onClick:async()=>{await livePreviews.stopAll();const name=await invoke<string>("qa_add_room",{username:realRoom.value});rooms.value=[name];visible.value=true;}},"测试公开房间"),
      h("button",{class:"border rounded px-2",onClick:async()=>{await livePreviews.stopAll();rooms.value=[...initialRooms];visible.value=true;}},"本地四路"),
    ]),
    visible.value?h("div",{class:"grid grid-cols-2 gap-4"},rooms.value.map(username=>h(StreamerCard,{key:username,streamer:{username,auto_record:false,added_at:"",is_online:true,is_recording:false,is_recordable:true,viewers:0,status:"公开秀",thumbnail_url:null}}))):h("p",{},"已离开列表，全部预览应停止"),
  ]);
}}).use(createPinia()).use(i18n).mount("#app");
