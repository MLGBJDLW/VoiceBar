import type { AppConfig } from './types';

const initial: AppConfig = {
  provider:{kind:'funasr',endpoint:'ws://localhost:10095',model:'paraformer',apiKey:'',appId:'',apiSecret:'',language:'zh'},
  correction:{enabled:false,endpoint:'https://api.openai.com/v1/chat/completions',apiKey:'',model:'gpt-4o-mini',prompt:'只校对语音转写中的错别字和标点，保留原意，只输出修正后的文本。'},
  appearance:{anchor:'top',effect:'ribbon',accent:'#a7e8cf',opacity:0.94,intensity:1,scale:1,motion:'full',fps:30,combine:true,offsetX:0,offsetY:0},
  injection:{mode:'final',method:'unicode',correctionAtEnd:false},hotkey:'Ctrl+Alt+Space',cancelHotkey:'Escape',micDevice:'',updateCheck:'auto',maxRecordingSecs:300,autostart:false,historyEnabled:false,hotwords:[]
};
let current = structuredClone(initial);
try {
  const saved = localStorage.getItem('voicebar.preview');
  if (saved) current = { ...initial, ...JSON.parse(saved) };
  // Clear credentials even from older browser previews.
  localStorage.removeItem('voicebar_config');
  current.provider.apiKey=''; current.provider.apiSecret=''; current.correction.apiKey='';
} catch {}
const handlers: Record<string, Set<(event:any)=>void>> = {};
export async function mockInvoke(cmd: string, args?: any): Promise<any> {
  switch(cmd) {
    case 'get_config': return structuredClone(current);
    case 'save_config': {
      current=structuredClone(args.config);
      const safe=structuredClone(current); safe.provider.apiKey='';safe.provider.apiSecret='';safe.correction.apiKey='';
      localStorage.setItem('voicebar.preview',JSON.stringify(safe));return;
    }
    case 'test_connection': throw new Error('浏览器预览无法测试系统连接，请在 VoiceBar 桌面版中测试。');
    case 'get_status': return {state:'idle',message:'浏览器预览 · 使用桌面版进行真实录音'};
    case 'get_transcript': return {stable:'',partial:'',corrected:false};
    case 'list_microphones': return [];
    case 'autostart_info': return {enabled:Boolean(current.autostart),command:'"C:\\Program Files\\VoiceBar\\VoiceBar.exe" --hidden',hint:'Ctrl+Alt+Space'};
    case 'get_runtime_info': return {version:'0.1.0',recording:false};
    // Browser preview: no installer, so report honestly that self-update is off.
    case 'update_support': return {selfUpdate:false,currentVersion:'0.1.0'};
    case 'check_for_update': return {currentVersion:'0.1.0',available:false,version:null,notes:null,date:null};
    case 'install_update': throw new Error('浏览器预览无法安装更新，请在安装版 VoiceBar 中操作。');
    case 'preview_overlay': window.open('/?overlay=1','voicebar-preview','width=560,height=210');return;
    case 'copy_transcript': return;
    case 'hide_overlay': window.close();return;
    case 'start_recording': throw new Error('当前为浏览器预览。请运行 VoiceBar.exe 进行真实录音和全局输入。');
    case 'stop_recording': case 'cancel_recording': return;
    default: throw new Error('浏览器预览不支持该操作');
  }
}
export async function mockListen(event:string,handler:(event:any)=>void) {
  (handlers[event] ||= new Set()).add(handler);return () => handlers[event].delete(handler);
}
