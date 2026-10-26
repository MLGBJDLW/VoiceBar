import type { AppConfig } from './types';
import { EffectRenderer } from './effects';
import { icons } from './icons';
import { displayAccel, eventToAccel, formatAccel, parseAccel } from './shortcut';

const native = !!(window as any).__TAURI_INTERNALS__;
let invoke: (command:string,args?:any)=>Promise<any>;
let listen: (event:string,handler:(event:any)=>void)=>Promise<()=>void>;
const svg = (path:string) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${path}</svg>`;
const copyIcon = svg('<rect x="8" y="8" width="12" height="13" rx="3"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/>');
const waveIcon = svg('<path d="M3 10v4m4-8v12m5-16v20m5-16v12m4-8v4"/>');
const checkIcon = svg('<path d="m5 12 4 4L19 6"/>');
const arrowIcon = svg('<path d="m9 5 7 7-7 7"/>');
const esc = (s:unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const $ = <T extends HTMLElement=HTMLElement>(id:string) => document.getElementById(id) as T;
function toast(message:string,error=false) {
  const t=document.createElement('div'); t.className='toast'+(error?' error':'');t.textContent=message;
  document.body.append(t);setTimeout(()=>t.remove(),4200);
}
async function call(command:string,args?:any) {
  try { return await invoke(command,args); } catch(e) { toast(String(e),true); }
}
function sameOrigin(a:string,b:string) { try{return new URL(a.trim()).origin===new URL(b.trim()).origin;}catch{return false;} }
const activeStates = ['connecting','listening','processing'];
const states:Record<string,string>={idle:'准备就绪',connecting:'正在连接',listening:'正在聆听',processing:'正在整理',done:'已完成',error:'需要留意'};
const providers:Record<string,{name:string,endpoint:string,model:string,stream:boolean,note:string}> = {
  funasr:{name:'FunASR · 本机模型',endpoint:'ws://localhost:10095',model:'paraformer',stream:true,note:'本地双遍识别：实时显示，离线遍校正。需先启动本机 FunASR WebSocket 服务。'},
  dashscope:{name:'阿里云 · 百炼 Paraformer',endpoint:'wss://dashscope.aliyuncs.com/api-ws/v1/inference',model:'paraformer-realtime-v2',stream:true,note:'Paraformer 双向流式识别。使用开通语音识别权限的 DashScope API Key。此协议与 Qwen Realtime 协议不同。'},
  'openai-realtime':{name:'OpenAI · Realtime',endpoint:'wss://api.openai.com/v1/realtime?intent=transcription',model:'gpt-live-transcribe',stream:true,note:'Realtime 转写，24 kHz PCM。支持逐步转写与最终确认；模型名称可自行更改。'},
  deepgram:{name:'Deepgram · Nova',endpoint:'wss://api.deepgram.com/v1/listen?model=nova-3&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&punctuate=true',model:'nova-3',stream:true,note:'双向流式识别。语言与模型可按服务开通情况调整。'},
  xfyun:{name:'科大讯飞 · 流式听写',endpoint:'wss://iat-api.xfyun.cn/v2/iat',model:'iat',stream:true,note:'需要 APPID、APIKey 和 APISecret。支持动态修正；传统听写单次会话通常有 60 秒限制。'},
  'openai-compatible':{name:'OpenAI 兼容 · 云端 / 本地',endpoint:'https://api.openai.com/v1/audio/transcriptions',model:'whisper-1',stream:false,note:'录完后转写。可配置 OpenAI、Groq、硅基流动及兼容的本地 Whisper 服务。须填完整的 /audio/transcriptions 地址。'},
  whispercpp:{name:'whisper.cpp · 本机模型',endpoint:'http://localhost:8080/inference',model:'whisper',stream:false,note:'录完后转写。先运行 whisper-server 并加载自己的 GGUF/GGML 模型；主程序不常驻加载模型。'}
};
const effects=[['ribbon','流动丝带','层叠声浪，轻盈舒展'],['spectrum','声纹频带','细密节拍，随声起伏'],['orbit','涟漪轨道','环形轨迹，温柔呼吸'],['bars','节奏音柱','简洁有力，清晰反馈']];
const anchors=['top-left','top','top-right','left','center','right','bottom-left','bottom','bottom-right'];
const anchorNames=['左上角','顶部','右上角','左侧','屏幕中央','右侧','左下角','底部','右下角'];

function barMarkup(overlay=false) {
  return `<div class="voice-shell" id="voice-shell">
    <div class="voice-pill">
      <span class="mic-glyph">${icons.mic}</span>
      <canvas id="voice-canvas" aria-label="声音波形"></canvas>
      <span class="voice-time" id="voice-time">00:00</span>
      <button class="icon-button stop-control" id="voice-stop" title="结束录音" aria-label="结束录音">${icons.stop}</button>
    </div>
    <div class="live-caption" id="live-caption">${overlay?'准备聆听':'把想说的话，变成眼前的文字。'}</div>
    <div class="overlay-meta"><span class="status-dot"></span><span id="live-status">准备就绪</span>${overlay?`<button class="text-icon" id="voice-settings" title="打开识别引擎设置" aria-label="打开识别引擎设置" hidden>设置</button><button class="text-icon" id="voice-copy" title="复制识别文本" aria-label="复制识别文本">${copyIcon}</button><button class="text-icon" id="voice-close" title="取消录音或关闭" aria-label="取消录音或关闭">${icons.x}</button>`:''}</div>
  </div>`;
}
async function overlay() {
  document.documentElement.classList.add('overlay-page');
  document.body.innerHTML=barMarkup(true);
  const renderer=new EffectRenderer($<HTMLCanvasElement>('voice-canvas'));
  let currentState='idle',timer:ReturnType<typeof setInterval>|undefined,startTime=0;
  function appearance(cfg:AppConfig) {
    document.documentElement.style.setProperty('--accent',cfg.appearance.accent);
    document.documentElement.style.setProperty('--pill-opacity',String(cfg.appearance.opacity));
    document.documentElement.style.setProperty('--overlay-scale',String(cfg.appearance.scale));
    const a=cfg.appearance.anchor;
    const x=a.includes('left')?'-36px':a.includes('right')?'36px':'0px';
    const y=a.startsWith('top')?'-24px':a.startsWith('bottom')?'24px':'0px';
    $('voice-shell').style.setProperty('--enter-x',x);$('voice-shell').style.setProperty('--enter-y',y);
    $('voice-shell').classList.toggle('reduced',cfg.appearance.motion==='reduced');
    renderer.updateConfig(cfg.appearance);
  }
  function onStatus(s:any) {
    currentState=s.state;
    $('live-status').textContent=s.message || states[s.state] || s.state;
    $('live-status').title=s.message||'';
    $('voice-settings').hidden=s.state!=='error';
    $('voice-shell').dataset.state=s.state;
    renderer.setActive(s.state==='listening');
    $<HTMLButtonElement>('voice-stop').disabled=!['connecting','listening'].includes(s.state);
    if(s.state==='listening'&&!timer) {
      startTime=Date.now();
      timer=setInterval(()=>{const sec=Math.floor((Date.now()-startTime)/1000);$('voice-time').textContent=`${String(Math.floor(sec/60)).padStart(2,'0')}:${String(sec%60).padStart(2,'0')}`;},1000);
    } else if(s.state!=='listening'&&timer) {clearInterval(timer);timer=undefined;}
  }
  function onTranscript(t:any) {
    $('live-caption').textContent=(t.stable||'')+(t.partial||'') || '把想说的话，变成眼前的文字。';
    $('live-caption').classList.toggle('corrected',!!t.corrected);
  }
  await listen('voicebar://config',e=>appearance(e.payload));
  await listen('voicebar://status',e=>onStatus(e.payload));
  await listen('voicebar://transcript',e=>onTranscript(e.payload));
  await listen('voicebar://level',e=>renderer.updateData(e.payload.rms,e.payload.bands));
  appearance(await invoke('get_config'));onStatus(await invoke('get_status'));onTranscript(await invoke('get_transcript'));
  $('voice-stop').onclick=()=>call('stop_recording');
  $('voice-copy').onclick=()=>call('copy_transcript').then(()=>toast('已复制'));
  $('voice-settings').onclick=()=>call('open_settings');
  $('voice-close').onclick=()=>call(activeStates.includes(currentState)?'cancel_recording':'hide_overlay');
  document.addEventListener('keydown',e=>{if(e.key==='Escape')call(activeStates.includes(currentState)?'cancel_recording':'hide_overlay');});
}
function field(path:string,label:string,type='text',extra='') {
  return `<label class="field"><span>${label}</span><input data-bind="${path}" type="${type}" ${extra}></label>`;
}
function select(path:string,label:string,options:[string,string][]) {
  return `<label class="field"><span>${label}</span><select data-bind="${path}">${options.map(([v,l])=>`<option value="${v}">${l}</option>`).join('')}</select></label>`;
}
function toggle(path:string,label:string,note:string) {
  return `<label class="toggle-row"><span><strong>${label}</strong><small>${note}</small></span><input data-bind="${path}" type="checkbox" role="switch"></label>`;
}
/** Press-to-record shortcut field. The hidden input keeps config binding intact. */
function shortcutField(path:string,label:string,hint:string) {
  return `<div class="field shortcut-field"><span>${label}</span>` +
    `<button type="button" class="shortcut-input" data-shortcut="${path}" aria-describedby="${path}-hint"><kbd></kbd><small>点击后按下组合键</small></button>` +
    `<input type="hidden" data-bind="${path}">` +
    `<span class="field-hint" id="${path}-hint">${hint}</span></div>`;
}
/** Shortcuts registered globally must be swallowed by the OS; recording one must not trigger it. */
function isRecordingShortcut(e:KeyboardEvent) {
  return e.ctrlKey || e.altKey || e.shiftKey || e.metaKey || /^F\d+$/.test(e.code || '');
}
async function settings() {
  let config:AppConfig=await invoke('get_config'), state='idle', demo=false;
  const mics:string[]=await invoke('list_microphones').catch(()=>[]);
  // The registry wins over the saved flag (installer updates, manual edits).
  const autostartInfo:{enabled:boolean;command:string}|null=await invoke('autostart_info').catch(()=>null);
  if(autostartInfo) config.autostart=autostartInfo.enabled;
  document.body.innerHTML=`<div class="app-layout">
    <aside class="sidebar">
      <a class="brand" href="#" aria-label="VoiceBar 首页"><span class="brand-symbol">${waveIcon}</span><span>VoiceBar<small>YOUR VOICE, IN FLOW</small></span></a>
      <div class="nav-label">工作空间</div>
      <nav aria-label="设置导航">${[['overview','概览',waveIcon],['appearance','外观与动效',icons.appearance],['engine','识别引擎',icons.mic],['input','输入与启动',icons.keyboard]].map(([id,label,icon],i)=>`<button class="nav-button ${i?'':'selected'}" data-tab="${id}">${icon}<span>${label}</span>${arrowIcon}</button>`).join('')}</nav>
      <div class="sidebar-bottom"><div class="native-status"><span class="status-dot"></span>${native?'桌面模式':'浏览器预览'}</div><small>声音有形，表达自由。</small><span class="version">VOICEBAR / APP_VERSION</span></div>
    </aside>
    <main class="workspace">
      <header class="topbar"><div class="breadcrumb">工作空间 <span>/</span> <b id="breadcrumb">概览</b></div><button class="button save-button" id="save">${checkIcon} 保存设置</button></header>
      <div class="page-content">
        ${native?'':`<div class="browser-notice">浏览器外观预览 · 真实录音和跨应用输入请使用桌面版。此处不会保存 API 密钥。</div>`}
        <div class="page-heading"><div><span class="eyebrow" id="page-eyebrow">A LITTLE LESS TYPING</span><h1 id="page-title">让思绪，自然成文。</h1><p id="page-description">按下快捷键，开口说话。其余的，交给 VoiceBar。</p></div><span class="page-mark">${waveIcon}</span></div>
        <section class="preview-stage" aria-label="语音条实时预览">
          <div class="stage-top"><span><i class="status-dot"></i> <span id="preview-label">语音条预览</span></span><button class="subtle-button" id="demo">播放动效演示 ${arrowIcon}</button></div>
          <div class="stage-guides" aria-hidden="true"><i></i><i></i><i></i></div>
          ${barMarkup()}
          <div class="stage-bottom"><span id="effect-name">流动丝带 / 顶部</span><button class="subtle-button" id="float-preview">查看桌面弹出效果 ${svg('<path d="M14 3h7v7m0-7-11 11M9 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-4"/>')}</button></div>
        </section>
        <section class="panel active" id="panel-overview">
          <div class="record-row"><div><h2>随时开口，即刻开始</h2><p>在任意可输入的位置按 <kbd id="hotkey-label"></kbd>，再按一次结束。</p></div><button class="button primary" id="record">${icons.mic} 试试真实录音</button></div>
          <div class="feature-grid">
            <article><span class="feature-number">01 / EVERYWHERE</span><h3>落在光标所在</h3><p>浏览器、桌面应用、终端。<br>用一个快捷键，接住每个想法。</p><div class="app-chips"><span>Chrome</span><span>App</span><span>Terminal</span></div></article>
            <article><span class="feature-number">02 / YOUR CHOICE</span><h3>选择你的声音引擎</h3><p>云端实时识别，或本机模型。<br>识别与校对，可以各选所长。</p><button class="inline-link" data-go="engine">配置识别引擎 ${arrowIcon}</button></article>
            <article><span class="feature-number">03 / QUIETLY THERE</span><h3>安静待命</h3><p>关闭设置后收起界面。<br>录音结束，释放音频与连接。</p><span class="quiet-line"><i></i> 按需唤醒的原生后台</span></article>
          </div>
          <div class="transcript-card"><div class="section-line"><span>本次识别 <small id="recognition-mode"></small></span><button class="subtle-button" id="copy-result">${copyIcon} 复制</button></div><p id="result-text">你的文字会出现在这里。暂定片段会随识别结果更新。</p><span class="result-status" id="result-status">尚未开始录音</span></div>
        </section>
        <section class="panel" id="panel-appearance">
          <div class="section-line"><h2>从你喜欢的位置出现</h2><span class="muted">跟随鼠标所在显示器</span></div>
          <div class="position-layout"><div class="monitor"><div class="anchor-grid" role="group" aria-label="语音条弹出方位">${anchors.map((a,i)=>`<button data-anchor="${a}" title="${anchorNames[i]}" aria-label="${anchorNames[i]}" aria-pressed="false"><i></i></button>`).join('')}</div><span class="monitor-label">YOUR SCREEN</span></div><div class="position-copy"><h3 id="anchor-name">顶部</h3><p>九个方位，始终留一点呼吸的空间。<br>支持多显示器与自定义位置偏移。</p><div class="field-grid compact">${field('appearance.offsetX','水平偏移 / px','number','min="-2000" max="2000"')}${field('appearance.offsetY','垂直偏移 / px','number','min="-2000" max="2000"')}</div></div></div>
          <h2 class="section-title">让声音，有自己的样子</h2>
          <div class="effect-grid">${effects.map(([id,name,note])=>`<button class="effect-card" data-effect="${id}" aria-pressed="false"><span class="effect-art effect-${id}" aria-hidden="true">${Array.from({length:13},(_,i)=>`<i style="--i:${i}"></i>`).join('')}</span><strong>${name}</strong><small>${note}</small><span class="effect-check">${checkIcon}</span></button>`).join('')}</div>
          <div class="field-grid controls-grid"><label class="field"><span>声浪颜色</span><span class="color-control"><input data-bind="appearance.accent" type="color"><b id="color-label"></b></span></label>${field('appearance.opacity','背景透明度','range','min="0.3" max="1" step="0.01"')}${field('appearance.intensity','声浪强度','range','min="0" max="3" step="0.1"')}${field('appearance.scale','语音条大小','range','min="0.65" max="1.8" step="0.05"')}</div>
          <p class="field-note">声浪会随麦克风音量实时起伏；「声浪强度」调整视觉响应幅度，不改变录音音量。静态预览可播放动效演示，真实录音时跟随你的声音。</p>
          ${toggle('appearance.combine','叠加柔光','为声音边缘增加轻柔的光晕。')}
          <div class="field-grid">${select('appearance.motion','动态偏好',[['full','完整动效'],['reduced','减少动态（静态音量反馈）']])}${select('appearance.fps','动效帧率',[['30','30 fps · 省电'],['60','60 fps · 流畅']])}</div>
        </section>
        <section class="panel" id="panel-engine">
          <div class="section-line"><h2>声音，由谁来理解</h2><span class="protocol-badge" id="protocol-badge"></span></div>
          ${select('provider.kind','识别协议',Object.entries(providers).map(([id,p])=>[id,p.name]))}
          <p class="engine-note" id="engine-note"></p>
          ${field('provider.endpoint','服务地址 / Endpoint','url','spellcheck="false"')}
          <div class="field-grid">${field('provider.model','模型名称','text','spellcheck="false"')}${field('provider.language','语言 / Language','text','placeholder="zh / en / auto"')}</div>
          ${field('provider.apiKey','API Key / Token','password','autocomplete="off" spellcheck="false"')}
          <div class="field-grid" id="xfyun-fields">${field('provider.appId','讯飞 APPID')}${field('provider.apiSecret','讯飞 APISecret','password','autocomplete="off"')}</div>
          <p class="field-note">密钥通过当前 Windows 账户加密保存；已保存的密钥以占位符显示，清空即删除。切换服务时需填写该服务自己的 Key。本机模型需先启动服务。</p>
          <div class="connection-check"><button class="button" id="test-connection" type="button">测试连接</button><p id="connection-result" role="status" aria-live="polite">测试当前表单中的服务，不录音、不保存设置。</p></div>
          <label class="field"><span>专有词与热词 / 每行一个</span><textarea id="hotwords" rows="3" placeholder="VoiceBar&#10;项目名&#10;专有名词"></textarea></label><p class="field-note">作为支持该能力的协议的识别提示；FunASR 使用热词，OpenAI 使用转写上下文。</p>
          <div class="section-divider"></div>
          ${toggle('correction.enabled','识别后，再校对一次','通过独立的语言模型修正错别字和标点；默认关闭。开启后转写文本将发送到下方服务。')}
          <div id="correction-fields">${field('correction.endpoint','校对服务 / OpenAI 兼容 Chat Completions','url','spellcheck="false"')}<div class="field-grid">${field('correction.model','校对模型')}${field('correction.apiKey','校对 API Key','password','autocomplete="off"')}</div><label class="field"><span>校对指令</span><textarea data-bind="correction.prompt" rows="3"></textarea></label></div>
        </section>
        <section class="panel" id="panel-input">
          <h2>一个快捷键，贯穿整个桌面</h2><p class="section-desc">点击输入位置，再唤起 VoiceBar。语音条会保持原应用的输入焦点。</p>
          <div class="field-grid">${shortcutField('hotkey','开始 / 结束录音','点击后直接按下组合键；支持 F 键单独使用。保存时立即生效。')}${shortcutField('cancelHotkey','取消本次录音','正在识别或连接时按下：丢弃本次结果、不写入文字。留空即禁用。')}</div>
          <div class="field-grid">${select('micDevice','麦克风设备',[['','跟随系统默认'],...mics.map(m=>[m,m] as [string,string])])}${field('maxRecordingSecs','单次录音上限 / 秒','number','min="5" max="900" step="5"')}</div>
          <div class="field-grid">${select('injection.mode','何时写入',[['final','结束后统一写入 · 适合校对'],['stable','确认片段逐段写入 · 更快'],['preview','只显示在语音条 · 手动复制']])}${select('injection.method','写入方式',[['unicode','Unicode 按键 · 保留剪贴板'],['clipboard','剪贴板粘贴 · 兼容部分应用']])}</div>
          ${toggle('autostart','开机时自动启动','只写入当前用户的启动项（无需管理员权限），登录后仅在托盘待命。')}
          <p class="field-note autostart-command" id="autostart-command" hidden></p>
          <div class="field-grid">${select('appearance.motion','动态偏好',[['full','完整动效'],['reduced','减少动态（静态音量反馈）']])}${select('appearance.fps','动效帧率',[['30','30 fps · 省电'],['60','60 fps · 流畅']])}</div>
          <div class="behavior-note"><span>${checkIcon}</span><div><strong>输入有分寸</strong><p>暂定文本只在语音条中更新。切换窗口后暂停自动写入，结果仍可复制；已经写入的文字不会被回删。终端中只输入文字，不自动按回车。</p></div></div>
          <p class="field-note">逐段写入模式下，最终校对结果会保留供复制。管理员窗口、密码框与部分自绘输入框可能限制模拟输入；以管理员身份运行的应用需要在同样权限下运行 VoiceBar。托盘图标单击即可打开设置。</p>
        </section>
        <footer class="page-footer"><span>为流畅表达，留出一点空间。</span><span>DESIGNED TO STAY OUT OF YOUR WAY</span></footer>
      </div>
    </main>
  </div>`;
  const renderer=new EffectRenderer($<HTMLCanvasElement>('voice-canvas'));
  const get=(path:string)=>path.split('.').reduce((v:any,k)=>v[k],config);
  const set=(path:string,value:any)=>{const keys=path.split('.');let v:any=config;keys.slice(0,-1).forEach(k=>v=v[k]);v[keys.at(-1)!]=value;};
  function fill() {
    document.querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement>('[data-bind]').forEach(el=>{
      const v=get(el.dataset.bind!);if(el instanceof HTMLInputElement&&el.type==='checkbox')el.checked=v;else el.value=String(v);
    });
    document.querySelectorAll<HTMLElement>('[data-shortcut]').forEach(button=>{
      button.querySelector('kbd')!.textContent=shortcutLabel(String(get(button.dataset.shortcut!)));
    });
    $<HTMLTextAreaElement>('hotwords').value=config.hotwords.join('\n');
  }
  function sync() {
    const a=config.appearance,p=providers[config.provider.kind]||providers.funasr;
    document.documentElement.style.setProperty('--accent',a.accent);
    document.documentElement.style.setProperty('--pill-opacity',String(a.opacity));
    renderer.updateConfig(a);$('hotkey-label').textContent=shortcutLabel(config.hotkey);
    $('effect-name').textContent=`${effects.find(e=>e[0]===a.effect)?.[1]} / ${anchorNames[anchors.indexOf(a.anchor)]}`;
    $('anchor-name').textContent=anchorNames[anchors.indexOf(a.anchor)];$('color-label').textContent=a.accent.toUpperCase();
    $('engine-note').textContent=p.note;$('protocol-badge').textContent=p.stream?'实时流式':'录完后转写';
    $('recognition-mode').textContent=p.stream?'实时预览':'录完后转写';
    $('xfyun-fields').hidden=config.provider.kind!=='xfyun';$('correction-fields').hidden=!config.correction.enabled;
    document.querySelector('[data-bind="provider.apiKey"]')!.closest('label')!.querySelector('span')!.textContent=
      ['funasr','whispercpp'].includes(config.provider.kind)?'访问 Token（仅服务启用鉴权时填写）':'当前服务的 API Key / Token';
    document.querySelectorAll<HTMLElement>('[data-anchor]').forEach(el=>el.setAttribute('aria-pressed',String(el.dataset.anchor===a.anchor)));
    document.querySelectorAll<HTMLElement>('[data-effect]').forEach(el=>el.setAttribute('aria-pressed',String(el.dataset.effect===a.effect)));
  }
  const tabs:Record<string,[string,string,string]>={
    overview:['概览','让思绪，自然成文。','按下快捷键，开口说话。其余的，交给 VoiceBar。'],
    appearance:['外观与动效','让声音，成为风景。','从出现的位置，到每一次起伏，都由你定义。'],
    engine:['识别引擎','让每一种表达，被听见。','选择本地或云端，为你的语言找到合适的引擎。'],
    input:['输入与启动','想说，就能写。','快捷键、录入方式与开机自启，都在这里。']};
  function tab(id:string) {
    const data=tabs[id];if(!data)return;
    document.querySelectorAll<HTMLElement>('[data-tab]').forEach(el=>el.classList.toggle('selected',el.dataset.tab===id));
    document.querySelectorAll('.panel').forEach(el=>el.classList.toggle('active',el.id==='panel-'+id));
    $('page-title').textContent=data[1];$('page-description').textContent=data[2];$('breadcrumb').textContent=data[0];
  }
  document.querySelectorAll<HTMLElement>('[data-tab]').forEach(el=>el.onclick=()=>tab(el.dataset.tab!));
  document.querySelectorAll<HTMLElement>('[data-go]').forEach(el=>el.onclick=()=>tab(el.dataset.go!));
  document.querySelector('.brand')!.addEventListener('click',e=>{e.preventDefault();tab('overview');});
  if(autostartInfo?.command){
    const note=$('autostart-command');
    note.hidden=false;
    note.textContent=`当前用户的启动项：${autostartInfo.command}。卸载或关闭后自动移除。`;
  }
  const prefix=native?'':'PREVIEW ';
  const versionLabel=document.querySelector('.version')!;
  if(native){invoke('get_runtime_info').then(i=>{versionLabel.textContent=`VOICEBAR / ${i.version||''}`;}).catch(()=>{versionLabel.textContent=`VOICEBAR / ${prefix}0.1.0`;});}
  else versionLabel.textContent=`VOICEBAR / 0.1.0`;
  function shortcutLabel(value:string) {
    const accel=parseAccel(value);
    return accel?displayAccel(accel):(value||'未设置');
  }
  let recordingShortcut:HTMLElement|null=null;
  function stopRecordingShortcut() {
    if(!recordingShortcut)return;
    const button=recordingShortcut;
    recordingShortcut=null;
    button.classList.remove('recording');
    button.querySelector('small')!.textContent='点击后按下组合键';
    void button.blur();
  }
  function commitShortcut(path:string,value:string) {
    const hidden=document.querySelector<HTMLInputElement>(`input[type="hidden"][data-bind="${path}"]`)!;
    hidden.value=value;
    hidden.dispatchEvent(new Event('input',{bubbles:true}));
  }
  document.querySelectorAll<HTMLElement>('[data-shortcut]').forEach(button=>{
    button.onclick=()=>{
      stopRecordingShortcut();
      recordingShortcut=button;
      button.classList.add('recording');
      button.querySelector('kbd')!.textContent='按下新的组合键…';
      button.querySelector('small')!.textContent='Esc 取消 · 点击别处放弃';
      button.focus();
    };
    button.onkeydown=e=>{
      if(recordingShortcut!==button)return;
      e.preventDefault();e.stopPropagation();
      const result=eventToAccel(e);
      if('reason' in result){button.querySelector('small')!.textContent=result.reason;return;}
      commitShortcut(button.dataset.shortcut!,formatAccel(result.accel));
      stopRecordingShortcut();
    };
    button.onblur=()=>{if(recordingShortcut===button)stopRecordingShortcut();};
  });
  // While a shortcut is being recorded, never let it reach the global handler.
  window.addEventListener('keydown',e=>{if(recordingShortcut)isRecordingShortcut(e)&&e.preventDefault();},true);
  document.querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement>('[data-bind]').forEach(el=>el.addEventListener('input',()=>{
    const path=el.dataset.bind!,old=get(path);
    set(path,el instanceof HTMLInputElement&&el.type==='checkbox'?el.checked:typeof old==='number'?Number(el.value):el.value);
    if(path==='provider.kind'){
      const p=providers[el.value];config.provider.endpoint=p.endpoint;config.provider.model=p.model;
      config.provider.apiKey='';config.provider.apiSecret='';config.provider.appId='';fill();
    }
    if(path==='provider.endpoint'&&config.provider.apiKey==='***saved***'&&!sameOrigin(String(old),el.value)){
      config.provider.apiKey='';config.provider.apiSecret='';fill();
    }
    if(path==='correction.endpoint'&&config.correction.apiKey==='***saved***'&&!sameOrigin(String(old),el.value)){
      config.correction.apiKey='';fill();
    }
    $('connection-result').textContent='配置已更改，请重新测试连接。';
    $('connection-result').dataset.state='';
    sync();$('save').classList.add('unsaved');
  }));
  $('hotwords').oninput=()=>{config.hotwords=$<HTMLTextAreaElement>('hotwords').value.split('\n').map(s=>s.trim()).filter(Boolean);$('save').classList.add('unsaved');};
  document.querySelectorAll<HTMLElement>('[data-anchor]').forEach(el=>el.onclick=()=>{config.appearance.anchor=el.dataset.anchor as any;sync();$('save').classList.add('unsaved');});
  document.querySelectorAll<HTMLElement>('[data-effect]').forEach(el=>el.onclick=()=>{config.appearance.effect=el.dataset.effect as any;sync();$('save').classList.add('unsaved');});
  let connectionCheck=0;
  $('test-connection').onclick=async()=>{
    const version=++connectionCheck;
    const form=JSON.stringify(config.provider),btn=$<HTMLButtonElement>('test-connection');
    btn.disabled=true;$('connection-result').textContent='正在测试连接…';$('connection-result').dataset.state='';
    try{
      const message=await invoke('test_connection',{config:structuredClone(config)});
      if(version===connectionCheck&&form===JSON.stringify(config.provider)){
        $('connection-result').textContent=message;$('connection-result').dataset.state='success';
      }
    }catch(e){
      if(version===connectionCheck&&form===JSON.stringify(config.provider)){
        $('connection-result').textContent=String(e);$('connection-result').dataset.state='error';
      }
    }finally{if(version===connectionCheck)btn.disabled=activeStates.includes(state);}
  };
  async function saveSettings(){
    await invoke('save_config',{config});config=await invoke('get_config');fill();sync();$('save').classList.remove('unsaved');
  }
  $('save').onclick=async()=>{
    const btn=$<HTMLButtonElement>('save');btn.disabled=true;
    try{await saveSettings();toast('设置已保存');}
    catch(e){toast(String(e),true);}finally{btn.disabled=activeStates.includes(state);}
  };
  $('demo').onclick=()=>{
    demo=!demo;renderer.setDemo(demo);$('preview-label').textContent=demo?'动效演示 · 合成声音数据':'语音条预览';
    $('demo').innerHTML=(demo?'停止演示':'播放动效演示')+arrowIcon;
  };
  $('float-preview').onclick=async()=>{
    try{await saveSettings();await invoke('preview_overlay');}catch(e){toast(String(e),true);}
  };
  $('record').onclick=async()=>{
    try{
      if(activeStates.includes(state)){await invoke('stop_recording');return;}
      if($('save').classList.contains('unsaved'))await saveSettings();
      await invoke('start_recording');
    }catch(e){toast(String(e),true);}
  };
  $('voice-stop').onclick=()=>call('stop_recording');
  $('copy-result').onclick=async()=>{
    if(native) await call('copy_transcript');else await navigator.clipboard.writeText($('result-text').textContent||'').catch(()=>{});
    toast('已复制');
  };
  function onStatus(s:any) {
    state=s.state;const busy=activeStates.includes(state);
    $('live-status').textContent=states[state]||state;$('voice-shell').dataset.state=state;
    $('result-status').textContent=s.message||states[state];$('save').toggleAttribute('disabled',busy);
    $<HTMLButtonElement>('test-connection').disabled=busy;
    $<HTMLButtonElement>('record').disabled=state==='processing';
    $('record').innerHTML=(busy?icons.stop:icons.mic)+(busy?'结束录音':'试试真实录音');
    $<HTMLButtonElement>('voice-stop').disabled=!['connecting','listening'].includes(state);
    renderer.setActive(state==='listening');
    if(busy&&demo){demo=false;renderer.setDemo(false);$('preview-label').textContent='真实麦克风输入';$('demo').innerHTML='播放动效演示'+arrowIcon;}
  }
  await listen('voicebar://status',e=>onStatus(e.payload));
  await listen('voicebar://level',e=>renderer.updateData(e.payload.rms,e.payload.bands));
  await listen('voicebar://transcript',e=>{
    const t=e.payload,text=(t.stable||'')+(t.partial||'');
    $('result-text').textContent=text||'你的文字会出现在这里。';$('live-caption').textContent=text||'把想说的话，变成眼前的文字。';
    $('result-text').classList.toggle('corrected',!!t.corrected);
  });
  fill();sync();onStatus(await invoke('get_status'));
}
async function main() {
  if(native){const core=await import('@tauri-apps/api/core'),event=await import('@tauri-apps/api/event');invoke=core.invoke;listen=event.listen;}
  else{const mock=await import('./mock');invoke=mock.mockInvoke;listen=mock.mockListen;}
  if(new URLSearchParams(location.search).get('overlay')==='1')await overlay();else await settings();
}
main().catch(e=>{document.body.innerHTML='<div class="startup-error"><h1>VoiceBar 启动遇到问题</h1><p></p></div>';document.querySelector('p')!.textContent=String(e);});
