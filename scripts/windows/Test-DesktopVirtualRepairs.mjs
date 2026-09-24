import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const root=process.argv[2];
const assets=path.join(root,'readable','webview','assets');
const initial=fs.readdirSync(assets).find(n=>/^app-initial-.*\.js$/.test(n));
const sharedFile=fs.readdirSync(assets).find(n=>/^app-shared-.*\.js$/.test(n));
assert(initial&&sharedFile,'Desktop source files are required');
const source=fs.readFileSync(path.join(assets,initial),'utf8');
const shared=await import(pathToFileURL(path.join(assets,sharedFile)));shared.RL();
const sample=JSON.parse(fs.readFileSync(0,'utf8'));
const symbols={};
for(const local of ['J','Z','Fr','Zr','$n','Hi','Ui','ca','ea','an','Mi']) {
 const match=source.match(new RegExp('\\b(\\w+) as '+local.replace('$','\\$')+','));assert(match,local);symbols[local]=shared[match[1]];
}
function slice(start,end){const a=source.indexOf(start),b=source.indexOf(end,a);assert(a>=0&&b>a,`${start} changed`);return source.slice(a,b);}
const models=vm.runInNewContext(slice('(zy = J().trim()','(pln = {').trim().replace(/,$/,'')+';({fln,lln})',symbols);
if(sample.models){const parsed=models.fln.parse(sample.models);assert(parsed.models.length>0);assert(parsed.models.every(Boolean));assert(parsed.versions.length>0);assert(parsed.versions.every(Boolean));for(const v of parsed.versions){assert(models.lln.safeParse(v).success);assert(v.slugs.length>0);assert(v.intelligence_presets.every(Boolean));}}
const beaconStart=source.indexOf('(Fk = J()'),beaconLast=source.indexOf('(Fmr = ea([',beaconStart),beaconEnd=source.indexOf('])));',beaconLast)+3;
const beacon=vm.runInNewContext(slice('function Pk(','function _mr(')+source.slice(beaconStart,beaconEnd)+';Fmr',{...symbols,vmr:{default:(obj,keys)=>Object.fromEntries(Object.entries(obj).filter(([k])=>!keys.includes(k)))}});
if(sample.beacons){const parsed=beacon.parse(sample.beacons.beacon_ui_response);assert(parsed.ui_info.title);assert(parsed.action_items.every(a=>['dismiss','open_url'].includes(a.action_v2.action_enum)));}
if(sample.hints){
 const prefix=source.match(/\(LYn = `([^`]+)`\)/)?.[1];assert(prefix);
 const read=slice('for (let t of [...r.system_hints, ...e.system_hints])','return a;');
 const result=vm.runInNewContext(slice('function IYn(','function sT(')+'(function(){'+read+'return a;})()',{LYn:prefix,r:sample.hints.connectors,e:sample.hints.plugins,i:[],a:{}});
 assert.equal(Object.keys(result).length,sample.hints.connectors.system_hints.length+sample.hints.plugins.system_hints.length);
}
if(sample.hints?.basic){
 const available=vm.runInNewContext(slice('function Kjr(','function cMr(')+';Kjr',{aMr:{search:{},picture_v2:{},tatertot:{}},iMr:{},oMr:{search:[],picture_v2:['image'],tatertot:['study']},rMr:['search','picture_v2','tatertot']});
 const items=available(sample.hints.basic.system_hints,{conversation:{isProjectMode:false,gizmoId:null}});assert.equal(items.length,sample.hints.basic.system_hints.length);assert(items.every(i=>i.title&&i.kind==='system_hint'));
}
if(sample.automation_pages){let calls=0;const read=vm.runInNewContext(slice('async function Gaa(','function Kaa(')+';Gaa',{Kaa:()=>false,oy:{safeGet:async(_path,options)=>{assert(calls<sample.automation_pages.length,'non-terminating cursor');const response=sample.automation_pages[calls++];if(calls>1)assert.equal(options.parameters.query.cursor,sample.automation_pages[calls-2].cursor);return response;}}});const result=await read('scheduled');assert.equal(calls,sample.automation_pages.length);assert(result.every(a=>a.is_enabled));}
if(sample.cloud_preferences){const sourcePrefs=fs.readFileSync(path.join(assets,fs.readdirSync(assets).find(n=>/^cloud-preferences-[a-f0-9]+\.js$/.test(n))),'utf8');const a=sourcePrefs.indexOf('function v('),b=sourcePrefs.indexOf('var x,',a);const validate=vm.runInNewContext(sourcePrefs.slice(a,b)+';v');assert.equal(validate(sample.cloud_preferences.branch_format,sample.cloud_schema.branch_format_max_length,sample.cloud_schema.branch_format_special_values),null);}
if(sample.events){const eventSchemas=vm.runInNewContext(slice('(SZr = Z({','(DZr = rf(').trim().replace(/,$/,'')+';({EZr,SZr,wZr})',symbols);for(const event of sample.events){assert(eventSchemas.EZr.safeParse(event.payload).success || eventSchemas.SZr.safeParse(event.payload).success || eventSchemas.wZr.safeParse(event.payload).success);}}
console.log('Actual Desktop model, hint, beacon, automation pagination, preference and conversation event readers accepted nonempty proxy samples.');
