import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import ts from '../node_modules/typescript/lib/typescript.js';

const source = new URL('../src/', import.meta.url);
function loadLocale(file: string, ui?: unknown): any {
 const exports = {};
 const code = ts.transpileModule(readFileSync(new URL(file, source), 'utf8'), {compilerOptions: {module: ts.ModuleKind.CommonJS}}).outputText;
 runInNewContext(code, {exports, require: (name: string) => {assert.equal(name, './ui'); return ui;}});
 return exports;
}

test('literal UI translation keys exist in both bundled languages', () => {
 const ui = loadLocale('locales/ui.ts');
 const locales = ['zh-CN', 'en-US'].map(code => loadLocale(`locales/${code}.ts`, ui).default);
 const files = ['views/HomeView.vue','views/RecordingsView.vue','views/SettingsView.vue','views/PostprocessView.vue','views/FinderView.vue','views/SetupView.vue','components/RecordingDetails.vue','components/RecordingPlayer.vue','components/ActionMenu.vue','components/AddStreamerDialog.vue','components/StreamerCard.vue','App.vue'];
 const missing: string[] = [];
 for(const file of files){
  const contents=readFileSync(new URL(file,source),'utf8');
  for(const match of contents.matchAll(/\bt\(\s*['"]([\w.]+)['"]/g)){
   for(const [i,locale] of locales.entries()){
    const value=match[1].split('.').reduce((obj,key)=>obj?.[key],locale);
    if(typeof value!=='string')missing.push(`${file}: ${match[1]} (${i===0?'zh-CN':'en-US'})`);
   }
  }
 }
 assert.deepEqual([...new Set(missing)],[]);
});
