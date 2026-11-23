import assert from "node:assert/strict";
import { test } from "node:test";
import { createExpandedWindowPin } from "../src/composables/useExpandedWindowPin.ts";

test("exit during an in-flight pin always writes unpin last", async () => {
  let finishPin!: () => void;
  const pinReply = new Promise<void>(resolve => { finishPin = resolve; });
  const nativeWrites: boolean[] = [];
  const pin = createExpandedWindowPin(async enabled => {
    nativeWrites.push(enabled);
    if (enabled) await pinReply;
  });

  const opening = pin.setPinned(true);
  await Promise.resolve();
  const exiting = pin.setPinned(false, true);
  assert.deepEqual(nativeWrites, [true]);
  assert.equal(pin.pinned.value, false);
  assert.equal(pin.pending.value, true);

  finishPin();
  await Promise.all([opening, exiting]);
  assert.deepEqual(nativeWrites, [true, false]);
  assert.equal(pin.pinned.value, false);
  assert.equal(pin.pending.value, false);
});

test("cleanup retries a temporary native failure and restores normal window level", async () => {
  let unpinFailures = 0;
  const pin = createExpandedWindowPin(async enabled => {
    if (!enabled && unpinFailures++ === 0) throw Error("Temporary native failure");
  });
  await pin.setPinned(true);
  await pin.setPinned(false, true);
  assert.equal(unpinFailures, 2);
  assert.equal(pin.pinned.value, false);
  assert.equal(pin.pending.value, false);
});
