import { test } from "node:test";
import assert from "node:assert/strict";
import { roomIdentity, matchesRoom, compareRooms } from "../src/lib/rooms.ts";

test("room identity agrees with backend platform and HTTPS boundaries", () => {
  assert.equal(roomIdentity("https://zh-hans.chaturbate.com/Alice/?x=1"), "chaturbate~alice");
  assert.equal(roomIdentity("Alice", "chaturbate"), "chaturbate~alice");
  assert.equal(roomIdentity("alice@xh"), "alice@xh");
  assert.equal(roomIdentity("https://mirror.test/alice/", "stripchat", "mirror.test"), "alice");
  for (const value of ["../alice", "http://stripchat.com/alice", "https://chaturbate.com.evil.test/alice", "https://a@chaturbate.com/alice", "https://chaturbate.com/alice/other", "chaturbate~", "https://stripchat.com:888/alice"]) assert.equal(roomIdentity(value), null, value);
  assert.equal(roomIdentity("bad-name", "chaturbate"), null);
});
test("independent filters preserve identities and sort online first", () => {
  const a = {username:"chaturbate~alice",is_online:true,is_recording:false};
  const b = {username:"alice",is_online:false,is_recording:true};
  const watching = new Set([a.username]);
  assert.equal(matchesRoom(a,"Chaturbate","watching",watching),true);
  assert.equal(matchesRoom(b,"alice","watching",watching),false);
  assert.equal(matchesRoom(b,"","recording",watching),true);
  assert.equal(matchesRoom(b,"","online",watching),false);
  assert.ok(compareRooms(a,b,true)<0);
  assert.equal(matchesRoom(a,"unknown","all",watching),false);
});
