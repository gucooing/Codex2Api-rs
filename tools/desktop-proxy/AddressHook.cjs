'use strict';

const fs = process.versions.electron ? require('original-fs') : require('node:fs');
const path = require('node:path');

function proxyPolicy(base) {
  const root = new URL(base);
  if (!['http:', 'https:'].includes(root.protocol) || root.username || root.password || root.search || root.hash) throw Error('Invalid proxy base URL.');
  const prefix = root.pathname.replace(/\/+$/, '') + '/backend-api';
  return Object.freeze({
    root: root.href,
    requestUrl(value) {
      let url;
      try { url = new URL(value); } catch { return value; }
      if (!['http:', 'https:', 'ws:', 'wss:'].includes(url.protocol)) return value;
      const official = ['chatgpt.com', 'chatgpt-staging.com', 'openai.com', 'openai.internal', 'oaistatic.com', 'oaiusercontent.com', 'oaistatsig.com']
        .some(host => url.hostname === host || url.hostname.endsWith('.' + host))
        || url.hostname === 'oaisidekickupdates.blob.core.windows.net';
      if (!official) return value;
      if (url.username || url.password) throw Error('Official request URL contains embedded credentials.');
      if (url.pathname === '/codex/desktop-auth' || (url.hostname === 'auth.openai.com' && url.pathname === '/oauth/authorize')) return this.loginUrl(url.href);
      const target = new URL(root);
      if (url.protocol === 'ws:' || url.protocol === 'wss:') target.protocol = root.protocol === 'https:' ? 'wss:' : 'ws:';
      const basePath = root.pathname.replace(/\/+$/, '');
      target.pathname = url.pathname === basePath || url.pathname.startsWith(basePath + '/') ? url.pathname : basePath + url.pathname;
      target.search = url.search;
      target.hash = url.hash;
      return target.href;
    },
    nativeConfig(saved, overrides = {}) {
      // A native provider with no base_url uses an official endpoint, even if
      // its id is "custom". openai_base_url only configures the built-in id.
      const config = { ...overrides };
      const rewriteConfiguredUrls=(value,parts=[])=>{
        if(!value||typeof value!=='object'||Array.isArray(value))return;
        for(const [key,entry] of Object.entries(value)){
          const path=[...parts,key];
          if(typeof entry==='string'&&(/(^|_)(url|endpoint|issuer)$/.test(key))){
            const mapped=this.requestUrl(entry);
            if(mapped!==entry){
              if(path.some(part=>!/^[a-zA-Z0-9_-]+$/.test(part)))throw Error('Unsupported native address configuration key.');
              config[path.join('.')]=mapped;
            }
          }else if(entry&&typeof entry==='object')rewriteConfiguredUrls(entry,path);
        }
      };
      rewriteConfiguredUrls(saved);rewriteConfiguredUrls(overrides);
      const providers = { ...saved?.model_providers, ...overrides.model_providers };
      for (const [key, value] of Object.entries(overrides)) {
        const match = /^model_providers\.([^.]+)$/.exec(key);
        if (match && value && typeof value === 'object') providers[match[1]] = { ...providers[match[1]], ...value };
      }
      config.chatgpt_base_url = new URL('https://chatgpt.com/backend-api').href;
      config.openai_base_url = new URL('https://chatgpt.com/backend-api/codex').href;
      for (const key of ['chatgpt_base_url', 'openai_base_url']) config[key] = this.requestUrl(config[key]);
      for (const [name, info] of Object.entries(providers)) {
        if (!info || typeof info !== 'object' || name === 'openai') continue;
        if (info.aws || name.startsWith('amazon-bedrock')) continue;
        const key = `model_providers.${name}.base_url`;
        const unquoted = `model_providers.${name}.base_url`;
        const base = overrides[key] ?? overrides[unquoted] ?? info.base_url;
        const original = base ?? (info.requires_openai_auth ? 'https://chatgpt.com/backend-api/codex' : 'https://api.openai.com/v1');
        const routed = this.requestUrl(original);
        if (routed !== original) {
          if(!/^[a-zA-Z0-9_-]+$/.test(name))throw Error("Unsupported native provider identifier for address routing.");
          delete config[unquoted];
          config[key] = routed;
        }
      }
      return config;
    },
    loginUrl(value) {
      let url;
      try { url = new URL(value); } catch { return value; }
      const authorizePath=root.pathname.replace(/\/+$/, '')+'/oauth/authorize';
      const wrapper=[root.origin,'https://chatgpt.com'].includes(url.origin)&&url.pathname==='/codex/desktop-auth';
      const authorize=(url.origin===root.origin&&url.pathname===authorizePath)
        ||(url.origin==='https://auth.openai.com'&&url.pathname==='/oauth/authorize');
      if(!wrapper&&!authorize)return value;
      return this.loginResponse('account/login/start',{authUrl:value}).authUrl;
    },
    loginRequest(method, value) {
      // The native callback otherwise redirects the external browser to the
      // official hosted success page, outside Desktop's network hooks.
      if (['account/login/start','account/sessions/add'].includes(method) && value?.type === 'chatgpt')
        return { ...value, useHostedLoginSuccessPage: false };
      return value;
    },
    loginResponse(method, value) {
      if (!['account/login/start','account/sessions/add'].includes(method) || !value?.authUrl) return value;
      const original = new URL(value.authUrl);
      let authorize = original;
      if ([root.origin,'https://chatgpt.com'].includes(original.origin) && original.pathname === '/codex/desktop-auth') {
        const nested = original.searchParams.getAll('authorize_url');
        if (nested.length !== 1) throw Error('Unexpected authorization URL.');
        authorize = new URL(nested[0]);
      }
      const local = root.pathname.replace(/\/+$/, '') + '/oauth/authorize';
      if (authorize.origin === 'https://auth.openai.com' && authorize.pathname === '/oauth/authorize') {
        authorize = new URL(local + authorize.search, root.origin);
      }
      if (authorize.origin !== root.origin || authorize.pathname !== local || authorize.username || authorize.password || authorize.hash) {
        throw Error('Authorization URL does not match the configured proxy.');
      }
      const wrapper = new URL('/codex/desktop-auth', root.origin);
      if (original.pathname === '/codex/desktop-auth') wrapper.search = original.search;
      wrapper.searchParams.set('authorize_url', authorize.href);
      return { ...value, authUrl: wrapper.href };
    },
    matches(value) {
      try {
        const url = new URL(value);
        return url.origin === root.origin && !url.username && !url.password && !url.hash
          && !/%(?:2f|5c)/i.test(url.pathname)
          && (url.pathname === prefix || url.pathname.startsWith(prefix + '/'));
      } catch { return false; }
    },
  });
}

function transform(source, kind) {
  if (kind === 'native') {
    const pattern = /this\.messageDelivery=new [\w$]+\(\{getConnection:\(\)=>this\.connection,[\s\S]{0,700}?transportKind:this\.options\.transport\.kind\}\)/g;
    if ([...source.matchAll(pattern)].length !== 1 || !source.includes('routeIncomingMessage(') || !source.includes('getUserSavedConfiguration(')) throw Error('Desktop native transport hook does not match this installed version.');
    source=source.replace(pattern, match => match + ',globalThis.__codex2apiDesktopHook.bindNative(this,this.messageDelivery)');
    const launch=/([\w$]+)=await ([\w$]+)\(this\.options,([\w$]+)\);if\(!\1\)throw Error\(`Unable to locate the Codex CLI binary/g;
    if([...source.matchAll(launch)].length!==1)throw Error('Desktop native launch boundary changed.');
    return source.replace(launch,(_,value,resolve,overrides)=>`${value}=await globalThis.__codex2apiDesktopHook.nativeLaunch(await ${resolve}(this.options,${overrides}),this.options.hostConfig.kind);if(!${value})throw Error(\`Unable to locate the Codex CLI binary`);
  }
  if (kind !== 'main') throw Error('Unknown Desktop transport module.');
  const pattern = /isDesktopAuthAllowedUrl\(([a-zA-Z_$][\w$]*)\)\{/g;
  const matches = [...source.matchAll(pattern)];
  if (matches.length !== 1) throw Error('Desktop ' + kind + ' hook does not match this installed version.');
  const match = matches[0];
  const addition = `if(globalThis.__codex2apiDesktopHook.policy.matches(${match[1]}))return true;`;
  return source.slice(0, match.index) + match[0] + addition + source.slice(match.index + match[0].length);
}

function readTargets(archive) {
  const fd = fs.openSync(archive, 'r');
  try {
    const prefix = Buffer.alloc(16); fs.readSync(fd, prefix, 0, 16, 0);
    const headerSize = prefix.readUInt32LE(4), jsonSize = prefix.readUInt32LE(12);
    if (jsonSize > 16 * 1024 * 1024 || jsonSize > headerSize) throw Error('Invalid ASAR header.');
    const bytes = Buffer.alloc(jsonSize); fs.readSync(fd, bytes, 0, jsonSize, 16);
    const entries = JSON.parse(bytes).files['.vite'].files.build.files;
    const targets = {};
    const sourceFor = entry => {
      if (entry.unpacked || entry.size > 64 * 1024 * 1024) throw Error('Unsupported desktop module.');
      const content = Buffer.alloc(entry.size);fs.readSync(fd, content, 0, content.length, 8 + headerSize + Number(entry.offset));
      return content.toString('utf8');
    };
    for (const kind of ['main', 'native']) {
      const candidates = Object.entries(entries).filter(([name, entry]) => name.endsWith('.js') &&
        (kind === 'main' ? name.startsWith('main-') : name.startsWith('src-') && sourceFor(entry).includes('this.messageDelivery=new')));
      if (candidates.length !== 1) throw Error('Unexpected desktop module layout.');
      const [name, entry] = candidates[0];
      if (entry.unpacked || entry.size > 64 * 1024 * 1024) throw Error('Unsupported desktop module.');
      const content = Buffer.alloc(entry.size);
      fs.readSync(fd, content, 0, content.length, 8 + headerSize + Number(entry.offset));
      transform(content.toString('utf8'), kind);
      targets[kind] = path.join(archive, '.vite', 'build', name);
    }
    // Verify the installed app-server still accepts these address-only overrides.
    const runtime = Object.entries(entries).filter(([name]) => name.startsWith('src-') && name.endsWith('.js'));
    const required = ['CODEX_API_BASE_URL','CODEX_APP_SERVER_CHATGPT_BASE_URL','CODEX_APP_SERVER_OPENAI_BASE_URL',
      'CODEX_APP_SERVER_LOGIN_ISSUER','CODEX_REFRESH_TOKEN_URL_OVERRIDE','CODEX_REVOKE_TOKEN_URL_OVERRIDE'];
    const found = new Set();
    for (const [name,entry] of [...runtime,...Object.entries(entries).filter(([name])=>/^(main|bootstrap)-.*\.js$/.test(name))]) {
      if(entry.unpacked || entry.size > 64*1024*1024) continue;
      const bytes=Buffer.alloc(entry.size);fs.readSync(fd,bytes,0,bytes.length,8+headerSize+Number(entry.offset));
      const source=bytes.toString('utf8');for(const key of required)if(source.includes(key))found.add(key);
    }
    if(required.some(key=>!found.has(key)))throw Error('Installed Desktop address overrides changed.');
    return targets;
  } finally { fs.closeSync(fd); }
}

function bindNativeRouting(connection, delivery, policy) {
  if (connection.options.transport.kind !== 'stdio') return;
  const send = delivery.sendMessage;
  delivery.sendMessage = function(message, context) {
    const params = message?.params;
    if (params && typeof params === 'object') {
      const login = policy.loginRequest(message.method, params);
      if (login !== params) message = { ...message, params: login };
    }
    // One native transport boundary handles foreground, background and resume
    // configuration. Prompts, model selection, permissions and credentials are
    // never rewritten. Read the native client's effective config, not TOML guesses.
    if (!params || typeof params !== 'object' || (!Object.hasOwn(params, 'config') && !Object.hasOwn(params, 'modelProvider'))) return send.call(this, message, context);
    const transport = connection.connection;
    connection.getUserSavedConfiguration(params.cwd ?? undefined).then(saved => {
      if (connection.connection !== transport) throw Error('Native connection changed during address resolution.');
      return send.call(this, { ...message, params: { ...params, config: policy.nativeConfig(saved, params.config ?? {}) } }, context);
    }).catch(error => {
      connection.routeIncomingMessage({ id: message.id, error: { code: -32603, message: 'Proxy address routing failed: ' + error.message } });
    });
  };
}

async function routeNativeLaunch(options, hostKind, policy, load) {
  if (!options || hostKind !== 'local') return options;
  if (options.spawnCommand) throw Error('This native launch transport cannot yet apply proxy addresses safely.');
  // Ask the installed runtime to resolve its own system/user/profile config.
  // This short-lived read process is not a shim and never rewrites config.toml.
  const {spawn}=load('node:child_process');
  const saved=await new Promise((resolve,reject)=>{
    const child=spawn(options.executablePath,[...options.args,'-c','model_provider="openai"'],{cwd:options.cwd,env:options.env,stdio:['pipe','pipe','pipe'],windowsHide:true});
    let pending='',settled=false;
    const finish=(error,config)=>{if(settled)return;settled=true;clearTimeout(timer);child.stdin.end();child.stdout.destroy();child.stderr.destroy();if(child.exitCode===null)child.kill();if(error)reject(error);else resolve(config);};
    const timer=setTimeout(()=>finish(Error('Native configuration address probe timed out.')),12000);
    const send=request=>child.stdin.write(JSON.stringify(request)+'\n');
    child.on('error',error=>finish(error));
    child.stdin.on('error',error=>{if(!settled)finish(error);});
    child.on('exit',()=>{if(!settled)finish(Error('Native configuration address probe exited before returning configuration.'));});
    child.stderr.on('data',()=>{});
    child.stdout.on('data',chunk=>{
      pending+=chunk.toString('utf8');
      if(pending.length>8*1024*1024)return finish(Error('Native configuration response exceeded the limit.'));
      for(let newline;(newline=pending.indexOf('\n'))>=0;){
        const line=pending.slice(0,newline);pending=pending.slice(newline+1);let message;
        try{message=JSON.parse(line);}catch{return finish(Error('Native configuration response was not JSON-RPC.'));}
        if(message.id==='proxy-init'){
          if(message.error)return finish(Error('Native configuration initialization failed.'));
          send({method:'initialized'});send({id:'proxy-config',method:'config/read',params:{includeLayers:false,cwd:options.cwd??null}});
        }else if(message.id==='proxy-config'){
          if(message.error||!message.result?.config)return finish(Error('Native effective configuration could not be read.'));
          finish(null,message.result.config);
        }
      }
    });
    send({id:'proxy-init',method:'initialize',params:{clientInfo:{name:'codex2api_address_probe',version:'1'},capabilities:{experimentalApi:true}}});
  });
  const addresses=policy.nativeConfig(saved);
  const overrides=Object.entries(addresses).flatMap(([key,value])=>['-c',key+'='+JSON.stringify(value)]);
  return {...options,args:[...options.args,...overrides]};
}

function routedHeaders(headers) {
  if (!headers) return headers;
  if (Array.isArray(headers)) {
    const result=[];
    for(let i=0;i<headers.length;i+=2) if(!/^(host|x-openai-account-routing-override)$/i.test(headers[i])) result.push(headers[i],headers[i+1]);
    return result;
  }
  if (typeof headers.entries === 'function') return Object.fromEntries([...headers.entries()].filter(([key])=> !/^(host|x-openai-account-routing-override)$/i.test(key)));
  return Object.fromEntries(Object.entries(headers).filter(([key])=> !/^(host|x-openai-account-routing-override)$/i.test(key)));
}

function routeRequestArguments(args, protocol, policy) {
  const [input, extra] = args;
  const hasUrl = typeof input === 'string' || input instanceof URL;
  const options = { ...(hasUrl ? typeof extra === 'object' ? extra : {} : input) };
  // Unix sockets / Windows named pipes are local IPC, not upstream HTTP routes.
  if (options.socketPath) return null;
  const callback = args.find(value => typeof value === 'function');
  let original;
  if (hasUrl) {
    original = new URL(input);
    if (options.hostname || options.host) original.hostname = options.hostname || options.host;
    if (options.port != null) original.port = options.port;
    if (options.path != null) {const parts=options.path.split('?');original.pathname=parts.shift();original.search=parts.length?'?'+parts.join('?'):'';}
  } else if (options.url) original = new URL(options.url);
  else {
    const host = options.hostname ?? options.host ?? 'localhost';
    const urlHost = !host.startsWith('[') && host.split(':').length > 2 ? `[${host}]` : host;
    original = new URL(`${options.protocol ?? protocol}//${urlHost}${options.port == null ? '' : ':' + options.port}${options.path ?? '/'}`);
  }
  const routed = policy.requestUrl(original.href);
  if (routed === original.href) return null;
  const target = new URL(routed);
  for (const key of ['host','hostname','port','protocol','path','url','servername','createConnection','socketPath','lookup']) delete options[key];
  Object.assign(options, { protocol: target.protocol, hostname: target.hostname, port: target.port || undefined, path: target.pathname + target.search, headers: routedHeaders(options.headers) });
  if (options.agent?.protocol && options.agent.protocol !== target.protocol) delete options.agent;
  return { target, args: callback ? [options, callback] : [options] };
}

function wrapFetch(original, receiver, policy) {
  return async function(input, init) {
    const value = typeof input === 'string' ? input : input instanceof URL ? input.href : input?.url ?? input?.href;
    let url;
    try { url = new URL(value); } catch { return original.call(receiver, input, init); }
    const service = new URL(policy.root), prefix = service.pathname.replace(/\/+$/, '');
    const serviceRequest = url.origin === service.origin && (url.pathname === prefix || url.pathname.startsWith(prefix + '/'));
    const loopback = url.hostname === 'localhost' || url.hostname.endsWith('.localhost') || url.hostname === '[::1]' || /^127(?:\.\d{1,3}){3}$/.test(url.hostname);
    // Preserve the exact input/options for browser bridges and internal protocols.
    // Reconstructing these as a web Request can discard native options or reject streams.
    if (!['http:', 'https:'].includes(url.protocol) || (loopback && !serviceRequest)) return original.call(receiver, input, init);
    let request = new Request(input, init);
    const mode = request.redirect;
    const extensions={};
    for(const key of ['session','useSessionCookies','bypassCustomProtocolHandlers','dispatcher'])if(init&&Object.hasOwn(init,key))extensions[key]=init[key];
    for (let count=0;count<=20;count++) {
      const routed = policy.requestUrl(request.url);
      if (routed !== request.url) {request = new Request(routed,request);request.headers.delete('host');request.headers.delete('x-openai-account-routing-override');}
      const retry = mode==='follow' && request.body ? request.clone() : request;
      const response = await original.call(receiver, new Request(request, { redirect:'manual' }), extensions);
      const location = response.headers.get('location');
      if (![301,302,303,307,308].includes(response.status) || !location || mode==='manual') return response;
      await response.body?.cancel();
      if (mode==='error' || count===20) throw new TypeError('Fetch redirect rejected.');
      const destination = policy.requestUrl(new URL(location,request.url).href);
      const headers = new Headers(retry.headers);
      if (new URL(destination).origin !== new URL(request.url).origin) for(const key of ['authorization','cookie','proxy-authorization']) headers.delete(key);
      const get = response.status===303 && !['GET','HEAD'].includes(request.method) || [301,302].includes(response.status) && request.method==='POST';
      if(get)for(const key of ['content-type','content-length','content-encoding','content-language','content-location'])headers.delete(key);
      request = new Request(destination,{method:get?'GET':retry.method,headers,body:get?undefined:retry.body,duplex:'half',signal:retry.signal,credentials:retry.credentials,redirect:mode});
    }
  };
}

function installRequestRouting(policy, load) {
  // These are transport APIs, not endpoint or feature-specific call sites.
  const http=load('node:http'), https=load('node:https');
  const originals={'http:':http.request,'https:':https.request};
  for(const [protocol,transport] of [['http:',http],['https:',https]]) {
    transport.request=function(...args){const routed=routeRequestArguments(args,protocol,policy);return routed?originals[routed.target.protocol].apply(routed.target.protocol==='http:'?http:https,routed.args):originals[protocol].apply(this,args);};
    transport.get=function(...args){const request=transport.request(...args);request.end();return request;};
  }
  if(globalThis.fetch)globalThis.fetch=wrapFetch(globalThis.fetch,globalThis,policy);
  if(globalThis.WebSocket)globalThis.WebSocket=new Proxy(globalThis.WebSocket,{construct(Target,args,newTarget){return Reflect.construct(Target,[policy.requestUrl(String(args[0])),...args.slice(1)],newTarget);}});
  const workerThreads=load('node:worker_threads'),OriginalWorker=workerThreads.Worker;
  // Node ignores --import for CommonJS eval workers. A normal --require preload
  // preserves their entry file, module type, argv and explicit worker options.
  const workerPrefix='--require=';
  let workerImport=process.execArgv.find(arg=>arg.startsWith(workerPrefix)&&/[\\/]codex2api-request-routing-[^\\/]+[\\/]preload\.cjs$/.test(arg));
  if(!workerImport){
    const files=load('node:fs'),paths=load('node:path');
    const directory=files.mkdtempSync(paths.join(load('node:os').tmpdir(),'codex2api-request-routing-'));
    const filename=paths.join(directory,'preload.cjs');
    const prelude="'use strict';const load=require;"+[proxyPolicy,routedHeaders,routeRequestArguments,wrapFetch,installRequestRouting].map(fn=>fn.toString()).join('\n')+
      '\ninstallRequestRouting(proxyPolicy('+JSON.stringify(policy.root)+'),load);';
    files.writeFileSync(filename,prelude,{encoding:'utf8',mode:0o600});
    process.once('exit',()=>{try{files.rmSync(directory,{recursive:true,force:true});}catch{}});
    workerImport=workerPrefix+filename;
  }
  workerThreads.Worker=class extends OriginalWorker {
    constructor(filename,options){
      const args=options?.execArgv??process.execArgv;
      super(filename,{...options,execArgv:[...args.filter(arg=>arg!==workerImport),workerImport]});
    }
  };
  const http2=load('node:http2'), connect=http2.connect;
  http2.connect=function(authority,...args){
    const original=new URL(authority),mapped=policy.requestUrl(original.href),session=connect.call(this,mapped,...args),request=session.request;
    session.request=function(headers={},...rest){const url=new URL(headers[':path']??'/',original);const next=policy.requestUrl(url.href);if(next===url.href)return request.call(this,headers,...rest);const target=new URL(next);return request.call(this,{...headers,':authority':target.host,':scheme':target.protocol.slice(0,-1),':path':target.pathname+target.search},...rest);};
    return session;
  };
  load('node:module').syncBuiltinESMExports();
  let electron;
  try { electron=load('electron'); } catch { return; }
  if(!electron?.net || !electron?.app) return;
  const netRequest=electron.net.request;
  electron.net.request=function(...args){const routed=routeRequestArguments(args,'https:',policy);return netRequest.apply(this,routed?routed.args:args);};
  if(electron.net.fetch)electron.net.fetch=wrapFetch(electron.net.fetch,electron.net,policy);
  const sessions=new WeakSet();
  const attach=session=>{
    if(sessions.has(session))return;sessions.add(session);
    const register=session.webRequest.onBeforeRequest.bind(session.webRequest);
    const matches=(patterns,value)=>patterns?.some(pattern=>{
      if(pattern==='<all_urls>')return true;
      const expression=pattern.replace(/[.+?^${}()|[\]\\]/g,'\\$&').replace(/\*/g,'.*');
      return new RegExp('^'+expression+'$').test(value);
    });
    session.webRequest.onBeforeRequest=function(filter,listener){
      if(typeof filter==='function'||filter===null){listener=filter;filter=null;}
      register({urls:['<all_urls>']},(details,callback)=>{
        const complete=(result={})=>{if(result.cancel)return callback(result);const original=result.redirectURL??details.url;const target=policy.requestUrl(original);callback(target===original?result:{...result,redirectURL:target});};
        if(listener&&(!filter||matches(filter.urls,details.url)))listener(details,complete);else complete();
      });
    };
    session.webRequest.onBeforeRequest(null);
    if(session.fetch)session.fetch=wrapFetch(session.fetch,session,policy);
  };
  electron.app.on('session-created',attach);
  electron.app.whenReady().then(()=>attach(electron.session.defaultSession));
}

function install(proxyRoot, targets, makePolicy, rewrite, load) {
  if (globalThis.__codex2apiDesktopHook) throw Error('Desktop hook is already installed.');
  const Module = load('node:module');
  const normalize = filename => filename.replace(/\\/g, '/').toLowerCase();
  const selected = Object.fromEntries(Object.entries(targets).map(([kind, filename]) => [normalize(filename), kind]));
  const applied = [];
  const policy = makePolicy(proxyRoot);
  const state = Object.freeze({ policy, applied, bindNative: (connection, delivery) => bindNativeRouting(connection, delivery, policy), nativeLaunch: (options,kind) => routeNativeLaunch(options,kind,policy,load) });
  Object.defineProperty(globalThis, '__codex2apiDesktopHook', { value: state });
  installRequestRouting(policy, load);
  const shell=load('electron').shell;
  const openExternal=shell.openExternal;
  shell.openExternal=function(url,...args){return openExternal.call(this,state.policy.requestUrl(url),...args);};
  const compile = Module.prototype._compile;
  Module.prototype._compile = function (source, filename, ...rest) {
    const kind = selected[normalize(filename)];
    if (kind) {
      source = rewrite(source, kind);
      if (!applied.includes(kind)) applied.push(kind);
      if (applied.length === 2) {
        Object.freeze(applied);
        Module.prototype._compile = compile;
      }
    }
    return compile.call(this, source, filename, ...rest);
  };
  return 'installed';
}

function isolateLauncherInspector(workerThreads, args, startupInspectorArg) {
  const inherited = args.filter(arg => arg !== startupInspectorArg);
  const OriginalWorker = workerThreads.Worker;
  // Electron can omit its inspector switch from process.execArgv altogether.
  // Node inherits its original native argv, not the mutable process.execArgv
  // array. Explicit worker options are necessary; preserve caller options and
  // all original arguments except the breakpoint inserted by this launcher.
  workerThreads.Worker = class extends OriginalWorker {
    constructor(filename, options) {
      super(filename, { ...options, execArgv: options?.execArgv === undefined ? inherited : options.execArgv });
    }
  };
  args.splice(0, args.length, ...inherited);
  return true;
}

module.exports = { proxyPolicy, transform, readTargets, install, isolateLauncherInspector, installRequestRouting, bindNativeRouting, routeNativeLaunch };
