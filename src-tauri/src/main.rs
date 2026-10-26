#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!("Production builds must embed the frontend: use scripts/build.ps1 or cargo build --release --features custom-protocol");

mod audio;
mod config;
mod platform;
mod providers;
mod session;
mod service_error;

use config::AppConfig;
use serde::Serialize;
use std::sync::{Arc,Mutex};
use tauri::{Emitter,Manager,WebviewUrl,WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt,Shortcut,ShortcutState};
use tokio::sync::mpsc;

#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Status { pub state:String,pub message:String }
pub struct Active { pub stop:mpsc::UnboundedSender<bool>,pub id:u64,pub cancel:Shortcut }
pub struct Runtime {
    pub config:Mutex<AppConfig>,pub status:Mutex<Status>,pub active:Mutex<Option<Active>>,
    pub transcript:Mutex<serde_json::Value>,pub counter:std::sync::atomic::AtomicU64,
}
impl Runtime {
    fn new(config:AppConfig)->Self{Self {config:Mutex::new(config),status:Mutex::new(Status{state:"idle".into(),message:"就绪".into()}),
        active:Mutex::new(None),transcript:Mutex::new(serde_json::json!({"stable":"","partial":"","corrected":false})),counter:std::sync::atomic::AtomicU64::new(0)}}
}
pub fn status(app:&tauri::AppHandle,state:&str,message:impl Into<String>) {
    let s=Status{state:state.into(),message:message.into()};
    *app.state::<Arc<Runtime>>().status.lock().unwrap()=s.clone();
    let _=app.emit("voicebar://status",s);
}
pub fn transcript(app:&tauri::AppHandle,stable:&str,partial:&str,corrected:bool) {
    let t=serde_json::json!({"stable":stable,"partial":partial,"corrected":corrected});
    *app.state::<Arc<Runtime>>().transcript.lock().unwrap()=t.clone();
    let _=app.emit("voicebar://transcript",t);
}
#[tauri::command]
fn get_config(state:tauri::State<'_,Arc<Runtime>>)->AppConfig{state.config.lock().unwrap().masked()}
#[tauri::command]
fn get_status(state:tauri::State<'_,Arc<Runtime>>)->Status{state.status.lock().unwrap().clone()}
#[tauri::command]
fn get_transcript(state:tauri::State<'_,Arc<Runtime>>)->serde_json::Value{state.transcript.lock().unwrap().clone()}
#[tauri::command]
fn save_config(app:tauri::AppHandle,state:tauri::State<'_,Arc<Runtime>>,mut config:AppConfig)->Result<(),String> {
    if state.active.lock().unwrap().is_some(){return Err("请先结束录音再更改配置".into());}
    let mut current=state.config.lock().unwrap();
    config.merge_secrets(&current);config.normalize();config.validate()?;
    let (toggle_text,cancel_text)=config.hotkeys();
    let toggle:Shortcut=toggle_text.parse().map_err(|e|format!("开始/结束快捷键格式无效：{e}"))?;
    let cancel:Option<Shortcut>=match &cancel_text {Some(text)=>Some(text.parse().map_err(|e|format!("取消快捷键格式无效：{e}"))?),None=>None};
    if cancel==Some(toggle) {return Err("取消快捷键不能与开始/结束快捷键相同".into());}
    let (old_toggle,old_cancel)=current.hotkeys();
    let toggle_changed=old_toggle!=toggle_text;
    let cancel_changed=old_cancel!=cancel_text;
    let shortcut_changed=toggle_changed||cancel_changed;
    let parse_or=|text:&Option<String>|->Option<Shortcut>{text.as_ref().and_then(|t|t.parse().ok())};
    let old_toggle_key:Option<Shortcut>=parse_or(&Some(old_toggle));
    let old_cancel_key:Option<Shortcut>=parse_or(&old_cancel);
    let autostart_changed=config.autostart!=current.autostart;
    if autostart_changed {platform::set_autostart(config.autostart)?;}
    if shortcut_changed {
        let rollback=||{
            let _=app.global_shortcut().unregister_all();
            for key in [old_toggle_key,old_cancel_key].into_iter().flatten() {let _=app.global_shortcut().register(key);}
            if autostart_changed {let _=platform::set_autostart(current.autostart);}
        };
        // Register the new set before releasing the old one; then verify both.
        if let Err(e)=app.global_shortcut().register(toggle) {rollback();return Err(format!("开始/结束快捷键被占用或不可用：{e}"));}
        if let Some(key)=cancel {
            if key!=toggle {
                if let Err(e)=app.global_shortcut().register(key) {rollback();return Err(format!("取消快捷键被占用或不可用：{e}"));}
            }
        }
        let missing=if cancel.is_some_and(|key|key!=toggle){2}else{1};
        let registered=[Some(toggle),cancel].into_iter().flatten()
            .filter(|key|app.global_shortcut().is_registered(*key)).count();
        if registered<missing {
            rollback();return Err("快捷键未能注册，可能已被其他程序占用".into());
        }
        if let Err(e)=config::save(&config::config_path(&app)?,&config) {rollback();return Err(e);}
        for key in [old_toggle_key,old_cancel_key].into_iter().flatten() {
            if Some(key)!=Some(toggle) && Some(key)!=cancel {let _=app.global_shortcut().unregister(key);}
        }
    } else if let Err(e)=config::save(&config::config_path(&app)?,&config) {
        if autostart_changed {let _=platform::set_autostart(current.autostart);}
        return Err(e);
    }
    // Keep the window title in sync so the active shortcut is visible even when
    // the UI is scrolled or covered; no native menu locale handling is needed.
    if shortcut_changed {
        if let Some(window)=app.get_webview_window("settings") {let _=window.set_title(&format!("VoiceBar · 开始/结束 {}",config.hotkey));}
    }
    *current=config;
    let _=app.emit("voicebar://config",current.masked());
    Ok(())
}
#[tauri::command]
async fn test_connection(state:tauri::State<'_,Arc<Runtime>>,mut config:AppConfig)->Result<String,String> {
    let current={
        if state.active.lock().unwrap().is_some(){return Err("请先结束录音再测试连接".into());}
        state.config.lock().unwrap().clone()
    };
    config.merge_secrets(&current);config.normalize();
    providers::check_connection(config.provider).await.map_err(|e|e.to_string())
}
#[tauri::command]
async fn list_microphones()->Result<Vec<String>,String> {
    tokio::task::spawn_blocking(audio::devices).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn start_recording(app:tauri::AppHandle)->Result<(),String>{start(app).await}
async fn start(app:tauri::AppHandle)->Result<(),String> {
    let runtime=app.state::<Arc<Runtime>>().inner().clone();
    let config=runtime.config.lock().unwrap().clone();
    if let Err(error)=config.validate_for_recording() {
        status(&app,"error",&error);
        let _=show_overlay(&app,&config);
        return Err(error);
    }
    let target=platform::foreground();
    let cancel=config.cancel_hotkey.parse::<Shortcut>().map_err(|e|format!("取消快捷键格式错误：{e}"))?;
    let id=runtime.counter.fetch_add(1,std::sync::atomic::Ordering::SeqCst)+1;
    let (stop,receiver)=mpsc::unbounded_channel();
    {
        let mut active=runtime.active.lock().unwrap();
        if active.is_some(){return Err("录音或转写正在进行".into());}
        *active=Some(Active{stop,id,cancel});
    }
    transcript(&app,"","",false);
    status(&app,"connecting","正在启动麦克风并连接识别引擎…");
    if let Err(e)=show_overlay(&app,&config) {
        runtime.active.lock().unwrap().take();status(&app,"error",&e);return Err(e);
    }
    tauri::async_runtime::spawn(async move {session::run(app,config,target,id,receiver).await;});
    Ok(())
}
#[tauri::command]
fn stop_recording(app:tauri::AppHandle)->Result<(),String>{signal_stop(&app,false)}
#[tauri::command]
fn cancel_recording(app:tauri::AppHandle)->Result<(),String>{signal_stop(&app,true)}
fn signal_stop(app:&tauri::AppHandle,cancel:bool)->Result<(),String>{
    let state=app.state::<Arc<Runtime>>();
    let active=state.active.lock().unwrap();
    if let Some(active)=active.as_ref() { let _=active.stop.send(cancel); }
    Ok(())
}
#[tauri::command]
fn preview_overlay(app:tauri::AppHandle)->Result<(),String>{
    let config=app.state::<Arc<Runtime>>().config.lock().unwrap().clone();
    if app.state::<Arc<Runtime>>().active.lock().unwrap().is_none(){status(&app,"idle","外观预览 · 按快捷键开始录音");}
    show_overlay(&app,&config)
}
#[tauri::command]
fn hide_overlay(app:tauri::AppHandle)->Result<(),String>{
    if app.state::<Arc<Runtime>>().active.lock().unwrap().is_some(){return Err("录音中请使用停止或取消按钮".into());}
    if let Some(w)=app.get_webview_window("overlay"){w.destroy().map_err(|e|e.to_string())?;}
    Ok(())
}
#[tauri::command]
fn copy_transcript(app:tauri::AppHandle)->Result<(),String>{
    let state=app.state::<Arc<Runtime>>();let t=state.transcript.lock().unwrap();
    let text=format!("{}{}",t["stable"].as_str().unwrap_or(""),t["partial"].as_str().unwrap_or(""));
    platform::copy_text(&text)
}
#[tauri::command]
fn autostart_info()->serde_json::Value {
    serde_json::json!({"enabled":platform::autostart_enabled().unwrap_or(false),
        "command":platform::autostart_command().unwrap_or_default(),
        "hint":config::default_hotkey_hint()})
}
#[tauri::command]
fn get_runtime_info(app:tauri::AppHandle)->serde_json::Value{
    let config_path=config::config_path(&app).ok();
    serde_json::json!({"pid":std::process::id(),"workingSetBytes":platform::working_set(std::process::id()),
        "configPath":config_path,"recording":app.state::<Arc<Runtime>>().active.lock().unwrap().is_some(),
        "version":env!("CARGO_PKG_VERSION")})
}
#[tauri::command]
fn open_settings(app:tauri::AppHandle)->Result<(),String>{settings(&app)}
fn settings(app:&tauri::AppHandle)->Result<(),String>{
    if let Some(w)=app.get_webview_window("settings"){w.show().map_err(|e|e.to_string())?;return w.set_focus().map_err(|e|e.to_string());}
    let hotkey=app.state::<Arc<Runtime>>().config.lock().unwrap().hotkey.clone();
    WebviewWindowBuilder::new(app,"settings",WebviewUrl::App("index.html".into()))
        .title(if hotkey.is_empty(){"VoiceBar · 让思绪自然成文".to_string()}else{format!("VoiceBar · 开始/结束 {hotkey}")})
        .inner_size(1140.,820.).min_inner_size(780.,600.)
        .center().build().map_err(|e|e.to_string())?;
    Ok(())
}
fn show_overlay(app:&tauri::AppHandle,config:&AppConfig)->Result<(),String>{
    let (width,height)=(560.0*config.appearance.scale,210.0*config.appearance.scale);
    let existing=app.get_webview_window("overlay");
    let w=match existing {
        Some(w)=>w,
        None=>WebviewWindowBuilder::new(app,"overlay",WebviewUrl::App("index.html?overlay=1".into()))
            .title("VoiceBar").inner_size(width,height).decorations(false).transparent(true)
            .shadow(false).always_on_top(true).skip_taskbar(true).resizable(false).focused(false)
            .focusable(false).visible(false).build().map_err(|e|e.to_string())?,
    };
    w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width,height))).map_err(|e|e.to_string())?;
    // Choose the monitor containing the pointer; falls back to primary.
    let cursor=app.cursor_position().map_err(|e|e.to_string())?;
    let monitor=app.monitor_from_point(cursor.x,cursor.y).ok().flatten()
        .or_else(||app.primary_monitor().ok().flatten()).ok_or("未找到显示器")?;
    let factor=monitor.scale_factor();
    let area=monitor.work_area();
    let left=area.position.x as f64/factor;let top=area.position.y as f64/factor;
    let mw=area.size.width as f64/factor;let mh=area.size.height as f64/factor;
    let a=config.appearance.anchor.as_str();let margin=16.;
    let x=if a.ends_with("left")||a=="left"{left+margin}else if a.ends_with("right")||a=="right"{left+mw-width-margin}else{left+(mw-width)/2.};
    let y=if a.starts_with("top"){top+margin}else if a.starts_with("bottom"){top+mh-height-margin}else{top+(mh-height)/2.};
    let x=(x+config.appearance.offset_x as f64).clamp(left,left+(mw-width).max(0.));
    let y=(y+config.appearance.offset_y as f64).clamp(top,top+(mh-height).max(0.));
    w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x,y))).map_err(|e|e.to_string())?;
    let _=app.emit_to("overlay","voicebar://config",config.masked());
    w.show().map_err(|e|e.to_string())?;
    Ok(())
}
fn tray_image()->tauri::image::Image<'static> {
    let mut rgba=vec![0u8;32*32*4];
    for y in 0..32usize{for x in 0..32usize{
        let i=(y*32+x)*4;let dx=x as f32-15.5;let dy=y as f32-15.5;
        if dx*dx+dy*dy<240. {rgba[i..i+4].copy_from_slice(&[24,32,30,255]);}
        let heights=[5,9,14,10,6];
        for (bar,h) in heights.iter().enumerate(){if x>=7+bar*4 && x<9+bar*4 && (y as i32-16).abs()<*h {rgba[i..i+4].copy_from_slice(&[167,232,207,255]);}}
    }}
    tauri::image::Image::new_owned(rgba,32,32)
}
fn startup_log(message:&str) {
    use std::io::Write;
    let base=std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir).join("VoiceBar").join("logs");
    if std::fs::create_dir_all(&base).is_ok() {
        if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(base.join("startup.log")) {
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let _=writeln!(file,"{now} pid={} {message}",std::process::id());
        }
    }
}
fn startup_error(message:&str) {
    startup_log(message);
    #[cfg(windows)] {
        use windows::{core::PCWSTR,Win32::UI::WindowsAndMessaging::{MessageBoxW,MB_ICONERROR,MB_OK}};
        let text:Vec<u16>=format!("{message}\n\n启动日志：%LOCALAPPDATA%\\VoiceBar\\logs\\startup.log").encode_utf16().chain(Some(0)).collect();
        let title:Vec<u16>="VoiceBar 启动失败".encode_utf16().chain(Some(0)).collect();
        unsafe {MessageBoxW(None,PCWSTR(text.as_ptr()),PCWSTR(title.as_ptr()),MB_OK|MB_ICONERROR);}
    }
}
fn main() {
    std::panic::set_hook(Box::new(|info|startup_error(&format!("程序初始化异常：{info}"))));
    startup_log(&format!("starting version {} custom-protocol={}",env!("CARGO_PKG_VERSION"),cfg!(feature = "custom-protocol")));
    let args:Vec<String>=std::env::args().collect();
    let hidden=args.iter().any(|a|a=="--hidden");
    let app=tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app,args,_|{
            if args.iter().any(|a|a=="--toggle"){
                let app=app.clone();tauri::async_runtime::spawn(async move{
                    if app.state::<Arc<Runtime>>().active.lock().unwrap().is_some(){let _=signal_stop(&app,false);}else{let _=start(app).await;}
                });
            }else if args.iter().any(|a|a=="--cancel"){let _=signal_stop(app,true);}
            else {let _=settings(app);}
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app,shortcut,event|{
            if event.state!=ShortcutState::Pressed{return;}
            let shortcut=*shortcut;
            let app=app.clone();
            tauri::async_runtime::spawn(async move{
                let active=app.state::<Arc<Runtime>>().active.lock().unwrap().as_ref().map(|a|a.cancel);
                match active {
                    None=>{if let Err(e)=start(app.clone()).await{status(&app,"error",e);}},
                    Some(cancel) if cancel==shortcut=>{let _=signal_stop(&app,true);status(&app,"idle","已取消本次录音");},
                    Some(_)=>{let _=signal_stop(&app,false);},
                }
            });
        }).build())
        .setup(move|app|{
            startup_log("setup started");
            platform::set_app_id("studio.voicebar.desktop");
            let handle=app.handle();
            let path=config::config_path(handle).map_err(std::io::Error::other)?;
            let (config,load_error)=match config::load(&path){Ok(c)=>(c,None),Err(e)=>(AppConfig::default(),Some(e))};
            let config_for_tray=config.clone();
            let (toggle_text,cancel_text)=config.hotkeys();
            let shortcut:Shortcut=toggle_text.parse()?;
            let cancel:Option<Shortcut>=cancel_text.as_ref().and_then(|t|t.parse().ok());
            // The registry, not the persisted flag, is the source of truth: an
            // installer update or a manual registry edit can change it.
            let startup_on=match platform::autostart_enabled() {Ok(on)=>on,Err(_)=>config.autostart};
            if startup_on!=config.autostart {
                let mut fixed=config.clone();fixed.autostart=startup_on;
                let _=config::save(&path,&fixed);
            }
            app.manage(Arc::new(Runtime::new(config)));
            if let Err(e)=handle.global_shortcut().register(shortcut){status(handle,"error",format!("快捷键注册失败：{e}，请在设置中更改"));}
            if let Some(cancel)=cancel {
                if cancel!=shortcut {
                    if let Err(e)=handle.global_shortcut().register(cancel) {
                        status(handle,"error",format!("取消快捷键注册失败：{e}，请在设置中更改"));
                    }
                }
            }
            use tauri::menu::{Menu,MenuItem};
            let show=MenuItem::with_id(handle,"settings","打开设置",true,None::<&str>)?;
            let toggle=MenuItem::with_id(handle,"toggle","开始 / 结束语音输入",true,None::<&str>)?;
            let quit=MenuItem::with_id(handle,"quit","退出 VoiceBar",true,None::<&str>)?;
            let menu=Menu::with_items(handle,&[&show,&toggle,&quit])?;
            tauri::tray::TrayIconBuilder::with_id("voicebar").icon(tray_image())
                .tooltip(format!("VoiceBar · {} · 单击打开设置",config_for_tray.hotkey))
                .menu(&menu)
                .on_tray_icon_event(|tray,event|{
                    if let tauri::tray::TrayIconEvent::Click{button:tauri::tray::MouseButton::Left,button_state:tauri::tray::MouseButtonState::Up,..}=event {
                        let _=settings(tray.app_handle());
                    }
                })
                .on_menu_event(|app,event|match event.id.as_ref(){
                    "settings"=>{let _=settings(app);},
                    "toggle"=>{let app=app.clone();tauri::async_runtime::spawn(async move{
                        if app.state::<Arc<Runtime>>().active.lock().unwrap().is_some(){let _=signal_stop(&app,false);}else{let _=start(app).await;}
                    });},
                    "quit"=>{let _=signal_stop(app,true);app.exit(0);},_=>{}
                }).build(handle)?;
            startup_log("tray ready");
            if !hidden{
                startup_log("opening settings");
                settings(handle).map_err(std::io::Error::other)?;
                startup_log("settings ready");
            }
            if let Some(e)=load_error{status(handle,"error",format!("配置读取失败，原文件已保留：{e}"));}
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config,save_config,get_status,get_transcript,start_recording,
            stop_recording,cancel_recording,preview_overlay,hide_overlay,open_settings,list_microphones,copy_transcript,get_runtime_info,test_connection,autostart_info])
        .build(tauri::generate_context!());
    match app {
        Ok(app)=>app.run(|_,event|match event {
            tauri::RunEvent::Ready=>startup_log("event loop ready"),
            tauri::RunEvent::ExitRequested{code:None,api,..}=>api.prevent_exit(),
            _=>{}
        }),
        Err(e)=>startup_error(&format!("VoiceBar 初始化失败：{e}")),
    }
}
