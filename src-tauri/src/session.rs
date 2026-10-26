use crate::{audio,config::AppConfig,platform,providers,Runtime};
use std::{sync::Arc,time::Duration};
use tauri::Manager;
use tokio::sync::mpsc;

struct Delivery { target:isize, delivered:String, paused:bool, internal:bool }
impl Delivery {
    fn new(target:isize)->Self{Self{target,delivered:String::new(),paused:false,internal:platform::process_id(target)==std::process::id()}}
    async fn append(&mut self,text:&str,method:&str)->Result<(),String>{
        if self.internal||self.paused||text==self.delivered{return Ok(());}
        // Providers can revise a final segment. Never backspace into user work.
        let Some(suffix)=text.strip_prefix(&self.delivered) else {self.paused=true;return Err("识别引擎修订了已确认片段，自动写入暂停；请复制完整结果。".into());};
        if suffix.is_empty(){return Ok(());}
        if !platform::target_valid(self.target){self.paused=true;return Err("输入焦点已切换，自动写入暂停；文本保留在语音条中。".into());}
        // Hotkey modifiers may still be down when the first result arrives.
        for _ in 0..20 {
            if platform::modifiers_released(){break;}
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let target=self.target;let fragment=suffix.to_string();let clipboard=method=="clipboard";
        let result=tokio::task::spawn_blocking(move||if clipboard{platform::paste_text(target,&fragment)}else{platform::type_text(target,&fragment)})
            .await.map_err(|e|e.to_string())?;
        if let Err(e)=result{self.paused=true;return Err(e);}
        self.delivered=text.into();Ok(())
    }
}

pub async fn run(app:tauri::AppHandle,config:AppConfig,target:isize,id:u64,mut stop:mpsc::UnboundedReceiver<bool>){
    let runtime=app.state::<Arc<Runtime>>().inner().clone();
    let result=execute(&app,&config,target,&mut stop).await;
    {
        let mut active=runtime.active.lock().unwrap();
        if active.as_ref().map(|v|v.id)==Some(id){active.take();}
    }
    let auto_hide=matches!(&result,Ok(Outcome::Cancelled)|Ok(Outcome::Done{keep:false,..}));
    match result {
        Ok(Outcome::Cancelled)=>{
            crate::status(&app,"idle","已取消；已写入的确认文本会保留");
            crate::transcript(&app,"","",false);
        },
        Ok(Outcome::Done{message,..})=>crate::status(&app,"done",message),
        Err(e)=>crate::status(&app,"error",e),
    }
    // Successful sessions disappear after a short acknowledgement; errors stay
    // visible so the user can recover the transcript without losing work.
    if auto_hide {
        tokio::time::sleep(Duration::from_millis(2200)).await;
        let state=runtime.status.lock().unwrap().state.clone();
        if runtime.active.lock().unwrap().is_none()&&runtime.counter.load(std::sync::atomic::Ordering::SeqCst)==id&&["done","idle"].contains(&state.as_str()){
            if let Some(w)=app.get_webview_window("overlay"){let _=w.destroy();}
            crate::status(&app,"idle","就绪 · Ctrl+Alt+Space 开始");
        }
    }
}
enum Outcome {Cancelled,Done{message:String,keep:bool}}

async fn execute(app:&tauri::AppHandle,config:&AppConfig,target:isize,stop:&mut mpsc::UnboundedReceiver<bool>)->Result<Outcome,String>{
    let (audio_tx,audio_rx)=mpsc::channel(256);
    let (output_tx,mut output_rx)=mpsc::unbounded_channel();
    let (errors_tx,mut errors_rx)=mpsc::unbounded_channel();
    let device=config.mic_device.clone();let capture_app=app.clone();
    let mut capture=tokio::task::spawn_blocking(move||audio::Capture::start(device,audio_tx,errors_tx,capture_app))
        .await.map_err(|e|e.to_string())??;
    let provider_config=config.provider.clone();let hotwords=config.hotwords.clone();
    let mut provider=tokio::spawn(providers::run(provider_config,hotwords,audio_rx,output_tx));
    // Aborting this task also closes provider resources on cancellation.
    struct AbortOnDrop(tokio::task::AbortHandle);
    impl Drop for AbortOnDrop {fn drop(&mut self){self.0.abort();}}
    let _provider_guard=AbortOnDrop(provider.abort_handle());
    let mut delivery=Delivery::new(target);
    let mut current=providers::Transcript{stable:String::new(),partial:String::new()};
    crate::status(app,"listening",if delivery.internal{"正在录音 · 设置窗口内仅预览，不向配置表单输入"}else{"正在录音 · 等待识别服务响应"});
    let limit=if config.provider.kind=="xfyun"{config.max_recording_secs.min(58)}else{config.max_recording_secs};
    let max=tokio::time::sleep(Duration::from_secs(limit));
    tokio::pin!(max);
    let mut focus=tokio::time::interval(Duration::from_millis(100));
    let mut finished=false;
    loop {
        tokio::select!{
            stop_result=stop.recv()=>{
                let cancel=stop_result.unwrap_or(true);
                capture.stop();
                if cancel{provider.abort();return Ok(Outcome::Cancelled);}
                break;
            }
            _=&mut max=>{capture.stop();break;}
            Some(e)=errors_rx.recv()=>{capture.stop();provider.abort();return Err(e);}
            _=focus.tick()=>{
                if !delivery.internal&&!delivery.paused&&platform::foreground()!=target{
                    delivery.paused=true;
                    crate::status(app,"listening","焦点已切换，自动写入暂停；识别继续，可复制结果");
                }
            }
            Some(update)=output_rx.recv()=>{
                update_transcript(app,config,&mut delivery,&mut current,update).await;
            }
            result=&mut provider=>{
                capture.stop();
                result.map_err(|e|e.to_string())?.map_err(|e|format!("识别服务错误：{e}"))?;
                finished=true;break;
            }
        }
    }
    capture.stop();
    crate::status(app,"processing","正在收尾并确认最后的文字…");
    if !finished{
        let wait=if ["openai-compatible","whispercpp"].contains(&config.provider.kind.as_str()){125}else{25};
        let deadline=tokio::time::sleep(Duration::from_secs(wait));tokio::pin!(deadline);
        loop {
            tokio::select! {
                command=stop.recv()=>{if command.unwrap_or(true){provider.abort();return Ok(Outcome::Cancelled);}}
                _=&mut deadline=>{provider.abort();return Err("识别收尾超时，已保留当前文本，请复制后重试。".into());}
                Some(update)=output_rx.recv()=>{update_transcript(app,config,&mut delivery,&mut current,update).await;}
                result=&mut provider=>{result.map_err(|e|e.to_string())?.map_err(|e|format!("识别服务错误：{e}"))?;break;}
            }
        }
    }
    while let Ok(update)=output_rx.try_recv(){update_transcript(app,config,&mut delivery,&mut current,update).await;}
    let original=format!("{}{}",current.stable,current.partial);
    if original.trim().is_empty(){return Ok(Outcome::Done{message:"未识别到文字，请检查麦克风或识别服务".into(),keep:true});}
    let mut final_text=original.clone();
    let mut correction_warning=None;
    if config.correction.enabled {
        crate::status(app,"processing","正在校对文字…");
        let corrected=tokio::select! {
            result=correct(config,&original)=>result,
            _=async {loop{if stop.recv().await.unwrap_or(true){break;}}}=>return Ok(Outcome::Cancelled),
        };
        match corrected{
            Ok(text)=>{final_text=text;crate::transcript(app,&final_text,"",true);}
            Err(e)=>{correction_warning=Some(e);crate::transcript(app,&original,"",false);}
        }
    }else{crate::transcript(app,&original,"",false);}
    let mut message:String;
    if config.injection.mode=="final"{
        if let Err(e)=delivery.append(&final_text,&config.injection.method).await{message=e;}
        else if delivery.internal{message="识别完成 · 设置窗口内仅预览，可复制结果".into();}
        else if delivery.paused{message="识别完成 · 焦点已变化，请复制结果".into();}
        else {message="已写入原输入位置".into();}
    }else if config.injection.mode=="stable"{
        // Corrections are shown for copying; existing text is never erased.
        if let Err(e)=delivery.append(&original,&config.injection.method).await{message=e;}
        else if delivery.paused{message="识别完成 · 自动写入已暂停，请复制完整结果".into();}
        else if config.correction.enabled{message="确认片段已写入 · 校对结果可单独复制".into();}
        else{message="确认片段已写入".into();}
    }else{message="识别完成 · 仅预览，可复制文字".into();}
    if let Some(e)=correction_warning{message=format!("{message}；校对失败，保留原文：{e}");}
    // Keep un-injected results/errors visible for manual recovery.
    let keep=delivery.paused||delivery.internal||config.injection.mode=="preview"||config.correction.enabled&&config.injection.mode=="stable";
    Ok(Outcome::Done{message,keep})
}
async fn update_transcript(app:&tauri::AppHandle,config:&AppConfig,delivery:&mut Delivery,current:&mut providers::Transcript,update:providers::Transcript){
    crate::transcript(app,&update.stable,&update.partial,false);
    if config.injection.mode=="stable"{
        if let Err(e)=delivery.append(&update.stable,&config.injection.method).await{crate::status(app,"listening",e);}
    }
    *current=update;
}
async fn correct(config:&AppConfig,text:&str)->Result<String,String>{
    let mut builder=reqwest::Client::builder().timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none());
    // Local correction servers must not be routed through a system proxy either.
    if crate::config::is_loopback_endpoint(&config.correction.endpoint) {builder=builder.no_proxy();}
    let client=builder.build().map_err(|e|e.to_string())?;
    let mut request=client.post(&config.correction.endpoint)
        .json(&serde_json::json!({"model":config.correction.model,"temperature":0,"messages":[
            {"role":"system","content":config.correction.prompt},{"role":"user","content":text}]}));
    if !config.correction.api_key.is_empty(){request=request.bearer_auth(&config.correction.api_key);}
    let response=request.send().await.map_err(|e|crate::service_error::http_request("校对服务",&config.correction.endpoint,&e))?;
    if !response.status().is_success(){return Err(crate::service_error::http_status("校对服务",response.status().as_u16()));}
    let data:serde_json::Value=response.json().await.map_err(|_|"校对服务返回无效 JSON".to_string())?;
    let out=data["choices"][0]["message"]["content"].as_str().ok_or("校对 API 没有返回文字")?.trim().to_string();
    if out.is_empty()||out.chars().count()>text.chars().count().saturating_mul(3)+100{return Err("校对结果异常，已保留原文".into());}
    Ok(out)
}
