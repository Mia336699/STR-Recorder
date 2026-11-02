import { reactive, ref } from "vue";
import { call, on } from "@/lib/api";

type Phase = "connecting" | "ready" | "retrying" | "error" | "closed";
interface PreviewEvent { preview_id: string; username: string; state: Phase; message?: string }
export interface LivePreview { id: string; url: string; state: Phase; message?: string }
const entries = reactive(new Map<string, LivePreview>());
const audible = ref<string | null>(null);
let listener: Promise<() => void> | undefined;
let routeGeneration = 0;
function apply(event: PreviewEvent) {
  const entry = entries.get(event.username);
  if (!entry || entry.id !== event.preview_id || entry.state === "error") return;
  if (event.state === "closed") { entries.delete(event.username); if (audible.value === event.username) audible.value = null; }
  else { entry.state = event.state; entry.message = event.message; }
}
function ensureListener() {
  return listener ??= on("live-preview-state", event => apply(event as PreviewEvent));
}
async function stop(username: string) {
  const entry = entries.get(username);
  entries.delete(username);
  if (audible.value === username) audible.value = null;
  if (entry) await call("stop_live_preview", { previewId: entry.id }).catch(() => {});
}
async function start(username: string) {
  if (entries.has(username)) void stop(username);
  const id = crypto.randomUUID();
  const generation = routeGeneration;
  entries.set(username, { id, url: "", state: "connecting" });
  try {
    await ensureListener();
    if (generation !== routeGeneration || entries.get(username)?.id !== id) return;
    const result = await call<{ preview_id: string; playlist_url: string }>("start_live_preview", { username, requestId: id });
    const entry = entries.get(username);
    if (generation !== routeGeneration || entry?.id !== id) {
      void call("stop_live_preview", { previewId: id }); return;
    }
    entry.url = result.playlist_url;
    // Catch a ready/error event that arrived before the IPC result or listener registration.
    const status = await call<PreviewEvent | null>("get_live_preview_status", { previewId: id });
    if (status) apply(status);
  } catch {
    const entry = entries.get(username);
    if (entry?.id === id) { entry.state = "error"; entry.message = undefined; }
    void call("stop_live_preview", { previewId: id }).catch(() => {});
  }
}
function fail(username: string, id: string) {
  const entry = entries.get(username);
  if (entry?.id !== id) return;
  entry.state = "error"; entry.message = undefined;
  if (audible.value === username) audible.value = null;
  void call("stop_live_preview", { previewId: id }).catch(() => {});
}
async function stopAll() {
  routeGeneration++;
  const ids = [...entries.values()].map(e => e.id);
  entries.clear(); audible.value = null;
  // Stop by request id rather than a global stop: delayed old-page cleanup cannot kill new-page previews.
  const oldListener = listener; listener = undefined;
  await Promise.all(ids.map(previewId => call("stop_live_preview", { previewId }).catch(() => {})));
  if (oldListener) { const unlisten = await oldListener.catch(() => undefined); unlisten?.(); }
}
function toggleSound(username: string) { audible.value = audible.value === username ? null : username; }
export const livePreviews = { entries, audible, start, stop, stopAll, fail, toggleSound };
