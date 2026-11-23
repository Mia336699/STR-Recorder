import { getCurrentWindow } from "@tauri-apps/api/window";
import { readonly, ref } from "vue";

export function createExpandedWindowPin(setAlwaysOnTop: (enabled: boolean) => Promise<void>) {
  const pinned = ref(false);
  const pending = ref(false);
  let queue = Promise.resolve();
  let queued = 0;

  function setPinned(enabled: boolean, retry = false) {
    if (!queued && pinned.value === enabled) return Promise.resolve();

    queued++;
    pending.value = true;
    const operation = queue.then(async () => {
      try {
        if (pinned.value === enabled) return;
        try {
          await setAlwaysOnTop(enabled);
        } catch (error) {
          if (!retry) throw error;
          await setAlwaysOnTop(enabled);
        }
        pinned.value = enabled;
      } finally {
        pending.value = --queued > 0;
      }
    });
    // A failed pin must never prevent a later cleanup from running.
    queue = operation.catch(() => {});
    return operation;
  }

  return { pinned: readonly(pinned), pending: readonly(pending), setPinned };
}

// There is one native window, so all player instances share the same write queue.
export const expandedWindowPin = createExpandedWindowPin(
  enabled => getCurrentWindow().setAlwaysOnTop(enabled),
);
