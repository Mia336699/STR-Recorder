import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { setImmediate as tick } from 'node:timers/promises';
import ts from '../node_modules/typescript/lib/typescript.js';
const require=createRequire(new URL('../package.json',import.meta.url));
function makeStore(call: (command:string,args:any)=>Promise<unknown>){
 const pinia=require('pinia');pinia.setActivePinia(pinia.createPinia());
 const exports:any={};
 const code=ts.transpileModule(readFileSync(new URL('../src/stores/settings.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
 runInNewContext(code,{exports,setTimeout,clearTimeout,require:(name:string)=>name==='@/lib/api'?{call,on:async()=>()=>{}}:require(name)});
 return exports.useSettingsStore();
}
test('settings writes stay serialized and capture each submitted snapshot',async()=>{
 const writes:any[]=[];const release:Array<()=>void>=[];
 const store=makeStore(async(command,args)=>{assert.equal(command,'save_settings_cmd');writes.push(args.newSettings);await new Promise<void>(resolve=>release.push(resolve));});
 const input={...store.settings,max_recording_duration_secs:900};
 const first=store.saveSettings(input);
 input.max_recording_duration_secs=1800;
 const second=store.saveSettings(input);
 await tick();assert.equal(writes.length,1);assert.equal(writes[0].max_recording_duration_secs,900);
 release[0]();await first;await tick();assert.equal(writes.length,2);assert.equal(writes[1].max_recording_duration_secs,1800);
 release[1]();await second;await store.waitForSaves();
 assert.equal(store.settings.max_recording_duration_secs,1800);assert.equal(store.saveStatus,'saved');
});
test('failed settings retain a retryable draft and do not block later saves',async()=>{
 let fail=true;const store=makeStore(async()=>{if(fail)throw new Error('simulated IPC failure');});
 await assert.rejects(store.saveSettings({...store.settings,max_recording_duration_secs:3600}),/simulated IPC failure/);
 assert.equal(store.saveStatus,'error');assert.equal(store.failedDraft.max_recording_duration_secs,3600);
 fail=false;await store.saveSettings({...store.failedDraft,max_recording_duration_secs:1800});
 assert.equal(store.settings.max_recording_duration_secs,1800);assert.equal(store.failedDraft,null);assert.equal(store.saveError,null);assert.equal(store.saveStatus,'saved');
});
