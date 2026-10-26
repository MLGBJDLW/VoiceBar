use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::FromSample;
use std::sync::mpsc;
use tauri::Emitter;

pub fn devices()->Result<Vec<String>,String> {
    cpal::default_host().input_devices().map_err(|e|e.to_string())?
        .map(|d|d.name().map_err(|e|e.to_string())).collect()
}
pub struct Capture { stop:mpsc::Sender<()>, thread:Option<std::thread::JoinHandle<()>> }
impl Capture {
    pub fn start(device_name:String,audio:tokio::sync::mpsc::Sender<Vec<i16>>,errors:tokio::sync::mpsc::UnboundedSender<String>,app:tauri::AppHandle)->Result<Self,String> {
        let (stop,rx)=mpsc::channel();
        let (ready_tx,ready)=mpsc::sync_channel(1);
        let thread=std::thread::spawn(move|| {
            let result=(||->Result<cpal::Stream,String> {
                let host=cpal::default_host();
                let device=if device_name.is_empty(){host.default_input_device().ok_or("没有可用的麦克风")?}
                    else {host.input_devices().map_err(|e|e.to_string())?.find(|d|d.name().ok().as_deref()==Some(&device_name)).ok_or("所选麦克风已断开，请重新选择")?};
                let supported=device.default_input_config().map_err(|e|e.to_string())?;
                let config:cpal::StreamConfig=supported.clone().into();
                match supported.sample_format() {
                    cpal::SampleFormat::F32=>build::<f32>(&device,&config,audio,errors.clone(),app),
                    cpal::SampleFormat::I16=>build::<i16>(&device,&config,audio,errors.clone(),app),
                    cpal::SampleFormat::U16=>build::<u16>(&device,&config,audio,errors.clone(),app),
                    f=>Err(format!("暂不支持麦克风格式 {f:?}")),
                }
            })();
            match result {
                Ok(stream)=> {
                    if let Err(e)=stream.play() {let _=ready_tx.send(Err(e.to_string())); return;}
                    let _=ready_tx.send(Ok(()));
                    let _=rx.recv(); drop(stream);
                }
                Err(e)=>{let _=ready_tx.send(Err(e));}
            }
        });
        match ready.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(Ok(()))=>Ok(Self{stop,thread:Some(thread)}),
            Ok(Err(e))=>{let _=thread.join();Err(e)},
            Err(_)=>{let _=stop.send(());Err("麦克风启动超时，请检查 Windows 麦克风权限".into())}
        }
    }
    pub fn stop(&mut self) {let _=self.stop.send(());if let Some(t)=self.thread.take(){let _=t.join();}}
}
impl Drop for Capture {fn drop(&mut self){self.stop();}}

fn build<T>(device:&cpal::Device,config:&cpal::StreamConfig,audio:tokio::sync::mpsc::Sender<Vec<i16>>,
    errors:tokio::sync::mpsc::UnboundedSender<String>,app:tauri::AppHandle)->Result<cpal::Stream,String>
where T:cpal::SizedSample+Send+'static, f32:FromSample<T> {
    let channels=config.channels as usize;
    let mut converter=Converter::new(config.sample_rate.0);
    let stream_errors=errors.clone();
    let mut failed=false;
    device.build_input_stream(config,move|data:&[T],_|{
        if failed {return;}
        for frame in data.chunks_exact(channels) {
            let mono=frame.iter().map(|v|v.to_sample::<f32>()).sum::<f32>()/channels as f32;
            for sample in converter.push(mono) {
                if let Some(packet)=sample {
                    let power=packet.iter().map(|v|(*v as f32/32768.0).powi(2)).sum::<f32>()/packet.len() as f32;
                    let bands:Vec<f32>=packet.chunks(27).map(|c|c.iter().map(|v|v.abs_diff(0) as f32/32768.0).sum::<f32>()/c.len() as f32).collect();
                    let _=app.emit("voicebar://level",serde_json::json!({"rms":power.sqrt(),"bands":bands}));
                    if audio.try_send(packet).is_err() {failed=true;let _=errors.send("识别服务处理过慢，音频缓冲已满。录音已停止以避免丢失音频。".into());break;}
                }
            }
        }
    },move|e|{let _=stream_errors.send(format!("麦克风错误：{e}"));},None).map_err(|e|e.to_string())
}
// Stateful linear resampling; the phase survives callback boundaries.
struct Converter { ratio:f64,index:u64,next:f64,previous:f32,started:bool,packet:Vec<i16> }
impl Converter {
    fn new(rate:u32)->Self{Self{ratio:rate as f64/16000.0,index:0,next:0.0,previous:0.0,started:false,packet:Vec::with_capacity(640)}}
    fn push(&mut self,current:f32)->Vec<Option<Vec<i16>>> {
        let mut ready=Vec::new();
        if !self.started {self.previous=current;self.started=true;}
        while self.next<=self.index as f64 {
            let t=(self.next-(self.index as f64-1.0)).clamp(0.0,1.0) as f32;
            let sample=self.previous+(current-self.previous)*t;
            self.packet.push((sample.clamp(-1.0,1.0)*32767.0).round() as i16);
            if self.packet.len()==640 {ready.push(Some(std::mem::replace(&mut self.packet,Vec::with_capacity(640))));}
            self.next+=self.ratio;
        }
        self.index+=1;self.previous=current;ready
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn resample_count_and_continuity() {
        for rate in [16000,44100,48000] {
            let mut c=Converter::new(rate);let mut output=vec![];
            for _ in 0..rate {for p in c.push(0.25).into_iter().flatten(){output.extend(p);}}
            assert_eq!(output.len(),16000);
            assert!(output.iter().all(|&v|(v as i32-8192).abs()<=1));
        }
    }
}
