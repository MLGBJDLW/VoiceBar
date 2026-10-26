// Deterministic protocol fixture. It does not recognize speech or save audio.
// Use only for integration checks; production default points to your own ASR.
import http from 'node:http';
import crypto from 'node:crypto';
const port=Number(process.argv[2]||10095);
const fixtureText='VoiceBar 流式输入验证。';
const server=http.createServer((req,res)=>{res.writeHead(200,{'Content-Type':'text/plain; charset=utf-8'});res.end('VoiceBar protocol fixture; synthetic transcript only');});
function frame(text) {
  const body=Buffer.from(JSON.stringify(text));
  if(body.length<126)return Buffer.concat([Buffer.from([0x81,body.length]),body]);
  const h=Buffer.alloc(4);h[0]=0x81;h[1]=126;h.writeUInt16BE(body.length,2);return Buffer.concat([h,body]);
}
server.on('upgrade',(req,socket)=>{
  const key=req.headers['sec-websocket-key'];
  if(!key){socket.destroy();return;}
  const accept=crypto.createHash('sha1').update(key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
  socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+'\r\nSec-WebSocket-Protocol: binary\r\n\r\n');
  let data=Buffer.alloc(0),packets=0,closed=false;
  socket.on('error',()=>{});
  socket.on('data',chunk=>{
    data=Buffer.concat([data,chunk]);
    while(data.length>=2){
      const opcode=data[0]&15,masked=!!(data[1]&128);
      let size=data[1]&127,offset=2;
      if(size===126){if(data.length<4)return;size=data.readUInt16BE(2);offset=4;}
      if(size===127){if(data.length<10)return;size=Number(data.readBigUInt64BE(2));offset=10;}
      if(size>4*1024*1024){socket.destroy();return;}
      const header=offset+(masked?4:0);
      if(data.length<header+size)return;
      const body=Buffer.from(data.subarray(header,header+size));
      if(masked){const mask=data.subarray(offset,offset+4);for(let i=0;i<size;i++)body[i]^=mask[i%4];}
      data=data.subarray(header+size);
      if(opcode===8){socket.end(Buffer.from([0x88,0]));return;}
      if(opcode===2){
        packets++;
        if(packets===8)socket.write(frame({mode:'2pass-online',text:'VoiceBar ',is_final:false}));
        if(packets===15)socket.write(frame({mode:'2pass-online',text:'流式输入',is_final:false}));
        if(packets===25)socket.write(frame({mode:'2pass-offline',text:fixtureText,is_final:false}));
      }
      if(opcode===1){
        try{
          const msg=JSON.parse(body.toString());
          if(msg.is_speaking===false&&!closed){
            closed=true;
            socket.write(frame({mode:'2pass-offline',text:packets>=25?'':fixtureText,is_final:true}));
            process.stdout.write(JSON.stringify({synthetic:true,packets,audioBytes:packets*1280,final:fixtureText})+'\n');
          }
        }catch{}
      }
    }
  });
});
server.listen(port,'localhost',()=>process.stdout.write('Fixture ready at http://localhost:'+port+'/health (SYNTHETIC TRANSCRIPTS)\n'));
