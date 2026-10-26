//! Provider adapters share PCM16/16k input and complete stable + tentative output.
//! No service-specific protocol or credential reaches the renderer.
use crate::config::ProviderConfig;
use anyhow::{anyhow,bail,Context,Result};
use base64::{engine::general_purpose::STANDARD,Engine};
use futures_util::{SinkExt,StreamExt};
use serde_json::{json,Value};
use std::{collections::BTreeMap,time::{Duration,SystemTime}};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest,Message};

#[derive(Debug,Clone)]
pub struct Transcript {pub stable:String,pub partial:String}
#[derive(Default)]
struct TextState {
    stable:String,partial:String,fun_online:String,
    xf:BTreeMap<u64,String>,items:Vec<String>,openai:BTreeMap<String,(String,bool)>,
    last_segment:Option<(u64,String)>,
}
impl TextState {
    fn emit(&self,output:&mpsc::UnboundedSender<Transcript>){
        let _=output.send(Transcript{stable:self.stable.clone(),partial:self.partial.clone()});
    }
    fn append(&mut self,text:&str){self.stable.push_str(text);self.partial.clear();}
    fn parse(&mut self,kind:&str,v:&Value,ending:bool)->Result<bool>{
        match kind {
            "dashscope"=>{
                match v["header"]["event"].as_str().unwrap_or("") {
                    "task-failed"=>bail!("阿里云任务失败：{} {}",v["header"]["error_code"].as_str().unwrap_or(""),v["header"]["error_message"].as_str().unwrap_or("请检查模型权限")),
                    "result-generated"=>{
                        let s=&v["payload"]["output"]["sentence"];
                        if let Some(text)=s["text"].as_str(){
                            if s["sentence_end"].as_bool().or(s["is_sentence_end"].as_bool()).unwrap_or(false){
                                // sentence begin_time identifies a provider retry/final repetition.
                                let stamp=s["begin_time"].as_u64().unwrap_or(u64::MAX);
                                if self.last_segment.as_ref()!=Some(&(stamp,text.to_string())){
                                    self.append(text);self.last_segment=Some((stamp,text.into()));
                                }else{self.partial.clear();}
                            }else{self.partial=text.into();}
                        }
                    }
                    "task-finished"=>return Ok(true),_=>{}
                }
            }
            "deepgram"=>{
                match v["type"].as_str().unwrap_or("") {
                    "Results"=>{
                        let text=v["channel"]["alternatives"][0]["transcript"].as_str().unwrap_or("");
                        if v["is_final"].as_bool().unwrap_or(false) {
                            if !text.is_empty(){
                                if !self.stable.is_empty()&&!self.stable.ends_with(char::is_whitespace){self.stable.push(' ');}
                                self.append(text);
                            }else{self.partial.clear();}
                        }else{self.partial=text.into();}
                    }
                    "Error"=>bail!("Deepgram：{}",v["description"].as_str().unwrap_or("识别失败")),
                    "Metadata" if ending=>return Ok(true),_=>{}
                }
            }
            "funasr"=>{
                let text=v["text"].as_str().unwrap_or("");
                let mode=v["mode"].as_str().unwrap_or("");
                if mode=="2pass-offline"||mode=="offline" {
                    self.append(text);self.fun_online.clear();
                }else {
                    // FunASR 2pass-online sends incremental fragments, not a full sentence.
                    self.fun_online.push_str(text);self.partial=self.fun_online.clone();
                }
                if ending&&v["is_final"].as_bool().unwrap_or(false){
                    if !self.partial.is_empty(){let p=std::mem::take(&mut self.partial);self.stable.push_str(&p);}
                    return Ok(true);
                }
            }
            "xfyun"=>{
                let code=v["code"].as_i64().unwrap_or(0);
                if code!=0{bail!("讯飞错误 {}：{}",code,v["message"].as_str().unwrap_or("请检查密钥/服务权限"));}
                let r=&v["data"]["result"];
                if let Some(sn)=r["sn"].as_u64() {
                    let text=r["ws"].as_array().map(|words|words.iter().filter_map(|w|w["cw"][0]["w"].as_str()).collect::<String>()).unwrap_or_default();
                    if r["pgs"]=="rpl" {
                        if let Some(range)=r["rg"].as_array(){if range.len()==2{
                            let start=range[0].as_u64().unwrap_or(0);let end=range[1].as_u64().unwrap_or(0);
                            self.xf.retain(|&n,_|n<start||n>end);
                        }}
                    }
                    self.xf.insert(sn,text);self.partial=self.xf.values().cloned().collect();
                }
                if v["data"]["status"].as_u64()==Some(2){self.stable=std::mem::take(&mut self.partial);return Ok(true);}
            }
            "openai-realtime"=>{
                let event=v["type"].as_str().unwrap_or("");
                if event=="error"||event.ends_with("transcription.failed"){
                    bail!("OpenAI：{}",v["error"]["message"].as_str().unwrap_or("转写失败，请检查模型与权限"));
                }
                if event=="input_audio_buffer.committed" {
                    if let Some(id)=v["item_id"].as_str(){
                        self.ensure_item(id);
                        if let Some(prev)=v["previous_item_id"].as_str(){
                            if let Some(pos)=self.items.iter().position(|i|i==id){self.items.remove(pos);}
                            let pos=self.items.iter().position(|i|i==prev).map(|p|p+1).unwrap_or(self.items.len());
                            self.items.insert(pos,id.into());
                        }
                    }
                }
                if event=="conversation.item.input_audio_transcription.delta" {
                    let id=v["item_id"].as_str().unwrap_or("current");self.ensure_item(id);
                    self.openai.entry(id.into()).or_default().0.push_str(v["delta"].as_str().unwrap_or(""));
                }
                if event=="conversation.item.input_audio_transcription.completed" {
                    let id=v["item_id"].as_str().unwrap_or("current");self.ensure_item(id);
                    self.openai.insert(id.into(),(v["transcript"].as_str().unwrap_or("").into(),true));
                    self.rebuild_openai();
                    return Ok(ending&&self.items.iter().all(|i|self.openai.get(i).is_some_and(|(_,done)|*done)));
                }
                self.rebuild_openai();
            }
            _=>bail!("不支持的流式协议"),
        }
        Ok(false)
    }
    fn ensure_item(&mut self,id:&str){if !self.items.iter().any(|i|i==id){self.items.push(id.into());}self.openai.entry(id.into()).or_default();}
    fn rebuild_openai(&mut self){
        self.stable.clear();self.partial.clear();let mut pending=false;
        for id in &self.items {if let Some((text,done))=self.openai.get(id){
            if !*done{pending=true;} if pending{self.partial.push_str(text);}else{self.stable.push_str(text);}
        }}
    }
}
fn pcm_bytes(samples:&[i16])->Vec<u8>{samples.iter().flat_map(|v|v.to_le_bytes()).collect()}
fn upsample_24k(samples:&[i16],previous:&mut Option<i16>)->Vec<i16>{
    let mut out=Vec::with_capacity(samples.len()*3/2);
    // Native packet is 640 samples; 3:2 conversion preserves packet duration.
    for pair in samples.chunks(2){
        let a=pair[0];let b=*pair.get(1).unwrap_or(&a);
        let before=previous.unwrap_or(a);
        out.push(((2*before as i32+a as i32)/3) as i16);
        out.push(a);
        out.push(((a as i32+2*b as i32)/3) as i16);
        *previous=Some(b);
    }
    out
}
fn wav(samples:&[i16])->Vec<u8>{
    let bytes=pcm_bytes(samples);let size=bytes.len() as u32;let mut out=Vec::with_capacity(bytes.len()+44);
    out.extend(b"RIFF");out.extend((size+36).to_le_bytes());out.extend(b"WAVEfmt ");out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(16000u32.to_le_bytes());
    out.extend(32000u32.to_le_bytes());out.extend(2u16.to_le_bytes());out.extend(16u16.to_le_bytes());
    out.extend(b"data");out.extend(size.to_le_bytes());out.extend(bytes);out
}
fn xfyun_url(config:&ProviderConfig)->Result<String>{
    use hmac::{Hmac,Mac};use sha2::Sha256;
    let mut url=url::Url::parse(&config.endpoint)?;let host=url.host_str().ok_or_else(||anyhow!("讯飞地址缺少域名"))?.to_string();
    let date=httpdate::fmt_http_date(SystemTime::now());
    let content=format!("host: {host}\ndate: {date}\nGET {} HTTP/1.1",url.path());
    let mut mac=Hmac::<Sha256>::new_from_slice(config.api_secret.as_bytes()).map_err(|_|anyhow!("讯飞密钥格式错误"))?;
    mac.update(content.as_bytes());let signature=STANDARD.encode(mac.finalize().into_bytes());
    let auth=format!("api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{signature}\"",config.api_key);
    let existing:Vec<(String,String)>=url.query_pairs().filter(|(k,_)|!["authorization","date","host"].contains(&k.as_ref())).map(|(k,v)|(k.into(),v.into())).collect();
    url.query_pairs_mut().clear().extend_pairs(existing).append_pair("authorization",&STANDARD.encode(auth)).append_pair("date",&date).append_pair("host",&host);
    Ok(url.into())
}
async fn send_json<S>(sink:&mut S,value:Value)->Result<()> where S:futures_util::Sink<Message>+Unpin,S::Error:std::error::Error+Send+Sync+'static {
    tokio::time::timeout(Duration::from_secs(5),sink.send(Message::Text(value.to_string().into()))).await.context("发送超时")??;Ok(())
}

fn websocket_request(config:&ProviderConfig)->Result<tokio_tungstenite::tungstenite::http::Request<()>> {
    let mut endpoint=if config.kind=="xfyun"{xfyun_url(config)?}else{config.endpoint.clone()};
    if config.kind=="deepgram"{
        let mut url=url::Url::parse(&endpoint)?;
        let existing:Vec<(String,String)>=url.query_pairs().filter(|(k,_)|k!="language"&&k!="model").map(|(k,v)|(k.into(),v.into())).collect();
        url.query_pairs_mut().clear().extend_pairs(existing);
        if !config.model.is_empty(){url.query_pairs_mut().append_pair("model",&config.model);}
        if !config.language.is_empty()&&config.language!="auto"{url.query_pairs_mut().append_pair("language",&config.language);}
        endpoint=url.into();
    }
    let mut request=endpoint.into_client_request()?;
    if config.kind!="funasr"&&config.kind!="xfyun"{
        let scheme=if config.kind=="deepgram"{"Token"}else{"Bearer"};
        request.headers_mut().insert("Authorization",format!("{scheme} {}",config.api_key).parse().context("API Key 不能包含换行")?);
    }else if config.kind=="funasr"&&!config.api_key.is_empty(){
        request.headers_mut().insert("Authorization",format!("Bearer {}",config.api_key).parse()?);
    }
    if config.kind=="funasr"{request.headers_mut().insert("Sec-WebSocket-Protocol","binary".parse()?);}
    Ok(request)
}
async fn connect(config:&ProviderConfig)->Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
    let request=websocket_request(config)?;
    let (socket,_)=tokio::time::timeout(Duration::from_secs(10),tokio_tungstenite::connect_async(request))
        .await.map_err(|_|anyhow!(crate::service_error::timeout(config.name(),&config.endpoint)))?
        .map_err(|e|anyhow!(crate::service_error::websocket(config,&e)))?;
    Ok(socket)
}
fn safe_error(config:&ProviderConfig,error:anyhow::Error)->anyhow::Error {
    anyhow!(crate::service_error::redact(&format!("{error:#}"),&[&config.api_key,&config.api_secret],&config.endpoint))
}
pub async fn check_connection(mut config:ProviderConfig)->Result<String> {
    config.normalize();config.validate_for_use().map_err(|e|anyhow!(e))?;
    let result=async {
        if config.is_batch() {
            let client=reqwest::Client::builder().timeout(Duration::from_secs(10)).redirect(reqwest::redirect::Policy::none()).build()?;
            let mut request=client.get(&config.endpoint);
            if !config.api_key.is_empty(){request=request.bearer_auth(&config.api_key);}
            let response=request.send().await.map_err(|e|anyhow!(crate::service_error::http_request(config.name(),&config.endpoint,&e)))?;
            let code=response.status().as_u16();
            if !response.status().is_success()&&![400,405,422].contains(&code) {
                bail!(crate::service_error::http_status(config.name(),code));
            }
            Ok(format!("{} HTTP 端点已响应（HTTP {code}）；未发送音频，密钥、模型和转写能力仍需实际录音验证。",config.name()))
        } else {
            let mut socket=connect(&config).await?;
            let _=tokio::time::timeout(Duration::from_secs(1),socket.close(None)).await;
            Ok(format!("{} WebSocket 握手成功；未发送音频，模型权限和识别效果仍需实际录音验证。",config.name()))
        }
    }.await;
    result.map_err(|e|safe_error(&config,e))
}
pub async fn run(mut config:ProviderConfig,hotwords:Vec<String>,audio:mpsc::Receiver<Vec<i16>>,output:mpsc::UnboundedSender<Transcript>)->Result<()> {
    config.normalize();config.validate_for_use().map_err(|e|anyhow!(e))?;
    run_inner(config.clone(),hotwords,audio,output).await.map_err(|e|safe_error(&config,e))
}
async fn run_inner(config:ProviderConfig,hotwords:Vec<String>,mut audio:mpsc::Receiver<Vec<i16>>,output:mpsc::UnboundedSender<Transcript>)->Result<()> {
    if config.is_batch(){return batch(config,hotwords,audio,output).await;}
    let socket=connect(&config).await?;
    let (mut sink,mut source)=socket.split();let task_id=uuid::Uuid::new_v4().simple().to_string();
    let kind=config.kind.as_str();
    match kind {
        "dashscope"=>send_json(&mut sink,json!({"header":{"action":"run-task","task_id":task_id,"streaming":"duplex"},"payload":{"task_group":"audio","task":"asr","function":"recognition","model":config.model,"parameters":{"format":"pcm","sample_rate":16000,"disfluency_removal_enabled":false},"input":{}}})).await?,
        "funasr"=>{
            let weights:serde_json::Map<String,Value>=hotwords.iter().map(|w|(w.clone(),json!(20))).collect();
            send_json(&mut sink,json!({"mode":"2pass","chunk_size":[5,10,5],"chunk_interval":10,"audio_fs":16000,"wav_name":"voicebar","wav_format":"pcm","is_speaking":true,"itn":true,"hotwords":Value::Object(weights).to_string()})).await?;
        }
        "openai-realtime"=>{
            let mut tr=json!({"model":config.model,"prompt":hotwords.join(", ")});
            if !config.language.is_empty()&&config.language!="auto"{
                if config.model=="gpt-live-transcribe"{tr["languages"]=json!([config.language]);}
                else{tr["language"]=json!(config.language);}
            }
            send_json(&mut sink,json!({"type":"session.update","session":{"type":"transcription","audio":{"input":{"format":{"type":"audio/pcm","rate":24000},"transcription":tr,"turn_detection":null}}}})).await?;
        }
        _=>{}
    }
    let mut text=TextState::default();let mut ready=kind!="dashscope";let mut ending=false;
    let mut end_deadline=tokio::time::Instant::now()+Duration::from_secs(3600);
    let ready_deadline=tokio::time::Instant::now()+Duration::from_secs(10);
    let mut first_xf=true;let mut sample_count=0usize;let mut previous=None;
    let mut pacing=tokio::time::interval(Duration::from_millis(40));
    pacing.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            packet=audio.recv(),if ready&&!ending=>{
                match packet {
                    Some(samples)=>{
                        if kind=="xfyun"{
                            pacing.tick().await;
                            if sample_count>=16000*59 {bail!("科大讯飞单次听写上限为 60 秒，当前结果已保留；请结束并开始下一段。");}
                        }
                        sample_count+=samples.len();
                        let bytes=if kind=="openai-realtime"{pcm_bytes(&upsample_24k(&samples,&mut previous))}else{pcm_bytes(&samples)};
                        if kind=="openai-realtime"{send_json(&mut sink,json!({"type":"input_audio_buffer.append","audio":STANDARD.encode(bytes)})).await?;}
                        else if kind=="xfyun"{
                            let mut v=json!({"data":{"status":if first_xf{0}else{1},"format":"audio/L16;rate=16000","encoding":"raw","audio":STANDARD.encode(bytes)}});
                            if first_xf{
                                let language=if config.language=="zh"||config.language=="zh-CN"{"zh_cn"}else if config.language=="en"{"en_us"}else{&config.language};
                                v["common"]=json!({"app_id":config.app_id});
                                v["business"]=json!({"language":language,"domain":"iat","accent":"mandarin","dwa":"wpgs","vad_eos":10000});
                                first_xf=false;
                            }
                            send_json(&mut sink,v).await?;
                        }else{
                            tokio::time::timeout(Duration::from_secs(5),sink.send(Message::Binary(bytes.into()))).await.context("音频发送超时")??;
                        }
                    }
                    None=>{
                        ending=true;end_deadline=tokio::time::Instant::now()+Duration::from_secs(15);
                        match kind {
                            "dashscope"=>send_json(&mut sink,json!({"header":{"action":"finish-task","task_id":task_id,"streaming":"duplex"},"payload":{"input":{}}})).await?,
                            "funasr"=>send_json(&mut sink,json!({"is_speaking":false})).await?,
                            "deepgram"=>send_json(&mut sink,json!({"type":"CloseStream"})).await?,
                            "openai-realtime"=>{
                                if sample_count<1600 {bail!("录音太短，请至少说一小段话");}
                                send_json(&mut sink,json!({"type":"input_audio_buffer.commit"})).await?;
                            }
                            "xfyun"=>send_json(&mut sink,json!({"data":{"status":2,"format":"audio/L16;rate=16000","encoding":"raw","audio":""}})).await?,
                            _=>{}
                        }
                    }
                }
            }
            message=source.next()=>{
                match message {
                    Some(Ok(Message::Text(raw)))=>{
                        let v:Value=serde_json::from_str(&raw).context("识别服务返回无效 JSON")?;
                        if kind=="dashscope"&&v["header"]["event"]=="task-started"{ready=true;}
                        let done=text.parse(kind,&v,ending)?;text.emit(&output);
                        if done{
                            // End-of-utterance may arrive before stop (e.g. XFYun VAD).
                            let _=sink.close().await;return Ok(());
                        }
                    }
                    Some(Ok(Message::Ping(data)))=>{sink.send(Message::Pong(data)).await?;}
                    Some(Ok(Message::Close(_)))|None=>{
                        if ending&&kind=="deepgram"{return Ok(());}
                        bail!("识别服务提前关闭连接；当前文本已保留");
                    }
                    Some(Err(e))=>bail!("识别连接中断：{e}"),
                    _=>{}
                }
            }
            _=tokio::time::sleep_until(end_deadline),if ending=>bail!("等待最终识别结果超时；当前文本已保留"),
            _=tokio::time::sleep_until(ready_deadline),if !ready=>bail!("识别任务启动超时"),
        }
    }
}
async fn batch(config:ProviderConfig,hotwords:Vec<String>,mut audio:mpsc::Receiver<Vec<i16>>,output:mpsc::UnboundedSender<Transcript>)->Result<()>{
    let mut samples=Vec::new();
    while let Some(packet)=audio.recv().await{
        if samples.len()+packet.len()>16000*900{bail!("录音长度超出上限");}samples.extend(packet);
    }
    if samples.len()<1600{bail!("录音太短，未发送转写请求");}
    let file=reqwest::multipart::Part::bytes(wav(&samples)).file_name("voicebar.wav").mime_str("audio/wav")?;
    let mut form=reqwest::multipart::Form::new().part("file",file).text("response_format","json");
    if config.kind!="whispercpp"{form=form.text("model",config.model.clone());}
    if !config.language.is_empty()&&config.language!="auto"{form=form.text("language",config.language.clone());}
    if !hotwords.is_empty(){form=form.text("prompt",hotwords.join(", "));}
    let client=reqwest::Client::builder().timeout(Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).build()?;
    let mut request=client.post(&config.endpoint).multipart(form);
    if !config.api_key.is_empty(){request=request.bearer_auth(&config.api_key);}
    let response=request.send().await.map_err(|e|anyhow!(crate::service_error::http_request(config.name(),&config.endpoint,&e)))?;
    if !response.status().is_success(){bail!(crate::service_error::http_status(config.name(),response.status().as_u16()));}
    let value:Value=response.json().await.context("转写服务返回无效 JSON")?;
    let text=value["text"].as_str().ok_or_else(||anyhow!("转写服务没有返回 text 字段"))?;
    let _=output.send(Transcript{stable:text.into(),partial:String::new()});Ok(())
}
#[cfg(test)]
#[path="provider_connection_tests.rs"]
mod connection_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unavailable_local_service_explains_setup() {
        let listener=tokio::net::TcpListener::bind("localhost:0").await.unwrap();
        let endpoint=format!("ws://{}",listener.local_addr().unwrap());
        drop(listener);
        let mut c=ProviderConfig::default();c.endpoint=endpoint;
        let (_tx,rx)=mpsc::channel(1);let (out,_results)=mpsc::unbounded_channel();
        let error=run(c,vec![],rx,out).await.unwrap_err().to_string();
        assert!(error.contains("本机服务未启动")&&error.contains("识别引擎"),"{error}");
        assert!(!error.contains("IO error"),"{error}");
    }
    #[test] fn xfyun_dynamic_replacement(){
        let mut s=TextState::default();
        s.parse("xfyun",&json!({"data":{"status":1,"result":{"sn":1,"ws":[{"cw":[{"w":"科盐"}]}]}}}),false).unwrap();
        s.parse("xfyun",&json!({"data":{"status":2,"result":{"sn":2,"pgs":"rpl","rg":[1,1],"ws":[{"cw":[{"w":"科研"}]}]}}}),true).unwrap();
        assert_eq!(s.stable,"科研");assert!(s.partial.is_empty());
    }
    #[test] fn openai_completion_order(){
        let mut s=TextState::default();
        s.parse("openai-realtime",&json!({"type":"input_audio_buffer.committed","item_id":"a"}),false).unwrap();
        s.parse("openai-realtime",&json!({"type":"input_audio_buffer.committed","item_id":"b","previous_item_id":"a"}),false).unwrap();
        s.parse("openai-realtime",&json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"b","transcript":"世界"}),false).unwrap();
        assert!(s.stable.is_empty());
        s.parse("openai-realtime",&json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"你好"}),false).unwrap();
        assert_eq!(s.stable,"你好世界");
    }
    #[test] fn funasr_two_pass_corrects_tentative_only(){
        let mut s=TextState::default();
        s.parse("funasr",&json!({"mode":"2pass-online","text":"科"}),false).unwrap();
        s.parse("funasr",&json!({"mode":"2pass-online","text":"盐"}),false).unwrap();
        assert_eq!(s.partial,"科盐");
        s.parse("funasr",&json!({"mode":"2pass-offline","text":"科研。"}),false).unwrap();
        assert_eq!(s.stable,"科研。");assert!(s.partial.is_empty());
    }
    #[test] fn wave_header_is_pcm16_mono16k(){
        let bytes=wav(&[1,-2,300]);assert_eq!(&bytes[..4],b"RIFF");assert_eq!(bytes.len(),50);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()),16000);
    }
    #[tokio::test]
    async fn funasr_streams_before_audio_close_and_waits_for_final(){
        let listener=tokio::net::TcpListener::bind("localhost:0").await.unwrap();
        let endpoint=format!("ws://{}",listener.local_addr().unwrap());
        let server=tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.unwrap();
            let mut ws=tokio_tungstenite::accept_hdr_async(tcp,|_: &tokio_tungstenite::tungstenite::handshake::server::Request, mut response: tokio_tungstenite::tungstenite::handshake::server::Response| { response.headers_mut().insert("Sec-WebSocket-Protocol","binary".parse().unwrap()); Ok(response) }).await.unwrap();
            let header=ws.next().await.unwrap().unwrap();assert!(header.to_text().unwrap().contains("2pass"));
            let pcm=ws.next().await.unwrap().unwrap();assert_eq!(pcm.into_data().len(),1280);
            ws.send(Message::Text(json!({"mode":"2pass-online","text":"实时"}).to_string().into())).await.unwrap();
            let finish=ws.next().await.unwrap().unwrap();assert!(finish.to_text().unwrap().contains("false"));
            ws.send(Message::Text(json!({"mode":"2pass-offline","text":"实时文本。","is_final":true}).to_string().into())).await.unwrap();
        });
        let mut c=ProviderConfig::default();c.endpoint=endpoint;
        let(tx,rx)=mpsc::channel(4);let(out,mut results)=mpsc::unbounded_channel();
        let job=tokio::spawn(run(c,vec![],rx,out));tx.send(vec![0;640]).await.unwrap();
        let interim=tokio::time::timeout(Duration::from_secs(3),results.recv()).await.unwrap().unwrap();
        assert_eq!(interim.partial,"实时");drop(tx);
        job.await.unwrap().unwrap();server.await.unwrap();
        let final_text=results.recv().await.unwrap();assert_eq!(final_text.stable,"实时文本。");
    }
}
