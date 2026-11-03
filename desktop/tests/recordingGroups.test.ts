import { test } from "node:test";
import assert from "node:assert/strict";
import { groupRecordings, recordingName } from "../src/lib/recordingGroups.ts";
import type { RecordingFile } from "../src/types/recordings.ts";
const file=(part:number, overrides:Partial<RecordingFile>={}):RecordingFile=>({name:`a_b_20261102_233000_part${String(part).padStart(2,'0')}.mp4`,path:`/test/chaturbate/a_b/a_b_20261102_233000_part${String(part).padStart(2,'0')}.mp4`,size_bytes:10,started_at:new Date(Date.UTC(2026,10,2,15,30)+part*1800000).toISOString(),is_recording:false,record_duration_secs:null,video_duration_secs:1800,group_started_at:'2026-11-02T23:30:00+08:00',part_index:part,...overrides});
test('sixteen shuffled parts stay one group across midnight and sort 09 before 10',()=>{
 const groups=groupRecordings(Array.from({length:16},(_,i)=>file(16-i)));
 assert.equal(groups.length,1);assert.equal(groups[0].username,'a_b');assert.equal(groups[0].platform,'Chaturbate');
 assert.deepEqual(groups[0].files.map(f=>recordingName(f).part),Array.from({length:16},(_,i)=>i+1));assert.equal(groups[0].totalSize,160);
});
test('platforms, folders and restarted recording groups remain distinct',()=>{
 const first=file(1),sc=file(1,{path:'/test/stripchat/a_b/'+first.name});
 const later=file(1,{name:'a_b_20261103_090000_part01.mp4',path:'/test/chaturbate/a_b/a_b_20261103_090000_part01.mp4',group_started_at:'2026-11-03T09:00:00+08:00'});
 const groups=groupRecordings([first,sc,later]);assert.equal(groups.length,3);assert.equal(groups[0].files[0].name,later.name);
});
test('mixed old and new files do not misread the username or invent a group',()=>{
 const old=file(1,{name:'a_b_20261017_190000.mp4',path:'/test/chaturbate/a_b/a_b_20261017_190000.mp4',group_started_at:undefined,part_index:undefined});
 assert.equal(recordingName(old).username,'a_b');assert.equal(recordingName(old).part,null);
 assert.equal(groupRecordings([old,file(1),file(2)]).length,2);
 const recovered=file(2,{group_started_at:undefined,part_index:undefined});assert.equal(groupRecordings([file(1),recovered]).length,1);
 assert.equal(recordingName({...old,name:'a_b_part01_20261017_190000.mp4'}).username,'a_b');
});
