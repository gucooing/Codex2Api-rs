'use strict';
// Execute the installed Desktop schema and model picker, without modifying it.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
(async()=>{
 const archive=fs.readFileSync(process.argv[2]);
 const header=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12))),offset=8+archive.readUInt32LE(4);
 const assets=header.files.webview.files.assets.files;
 const read=name=>{const e=assets[name];assert(e,`Missing installed asset ${name}`);const start=offset+Number(e.offset);return archive.subarray(start,start+e.size).toString();};
 function unique(prefix){const names=Object.keys(assets).filter(name=>name.startsWith(prefix)&&name.endsWith('.js'));assert.equal(names.length,1,`Installed bundle changed: ${prefix}`);return names[0];}
 const data=text=>'data:text/javascript;base64,'+Buffer.from(text).toString('base64');
 const sharedName=unique('app-shared-'),sharedSource=read(sharedName);
 const runtime=sharedSource.match(/from["']\.\/(rolldown-runtime-[^"']+)["']/)?.[1];assert(runtime,'Installed runtime import changed');
 const shared=await import(data(sharedSource.replaceAll('./'+runtime,data(read(runtime)))));shared.RL();
 const initialName=unique('app-initial-'),renderer=read(initialName),symbols={};
 for(const local of ['J','Z','Fr','Zr','$n']){const match=renderer.match(new RegExp('\\b(\\w+) as '+local.replace('$','\\$')+'[,}]'));assert(match,`Installed schema import changed: ${local}`);symbols[local]=shared[match[1]];}
 function between(start,end){const a=renderer.indexOf(start),b=renderer.indexOf(end,a);assert(a>=0&&b>a,`Installed model contract changed: ${start}`);return renderer.slice(a,b);}
 const schema=between('tln=`work_dogfood_default:`','pln={')+between('pln={','})))()}function mln(')+';';
 const functions=['Ly','Xcn','Kcn','Gcn','qcn','Wcn','Ucn','Ycn','Zcn','Jcn','Icn'].map(name=>{const a=renderer.indexOf('function '+name+'('),b=renderer.indexOf('function ',a+9);assert(a>=0&&b>a,`Installed model selector changed: ${name}`);return renderer.slice(a,b);}).join('');
 const reader=vm.runInNewContext(schema+functions+';({parse:value=>fln.parse(value),select:Icn})',symbols);
 const input=JSON.parse(fs.readFileSync(0,'utf8'));
 const sample=input.catalog??input;
 const selected=reader.select(reader.parse(sample));
 const slugs=sample.models.map(model=>model.slug);
 assert.deepEqual(Array.from(new Set(Array.from(selected.options,option=>option.slug))).sort(),[...slugs].sort());
 assert.equal(selected.defaultModelSlug,sample.default_model_slug);
 for(const slug of input.basic_models??slugs){const config=selected.modelConfigBySlug[slug];assert.equal(config.thinkingEfforts,undefined);assert.equal(config.configurableThinkingEffort,undefined);}
 const empty=reader.select(reader.parse({...sample,models:[],versions:[],categories:[],internal_groups:[],slider_settings:[],default_model_slug:null}));assert.equal(empty.options.length,0);
 console.log(JSON.stringify({source:initialName,options:slugs,defaultModelSlug:selected.defaultModelSlug,emptyOptions:empty.options.length,reader:'actual Desktop schema and selector'}));
})().catch(error=>{console.error(`${error.name}: ${String(error.message).slice(0,500)}`);process.exitCode=1;});
