fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources/voicebar.rc");
        println!("cargo:rerun-if-changed=resources/voicebar.manifest");
        println!("cargo:rerun-if-changed=icons/icon.ico");
        embed_resource::compile("resources/voicebar.rc", embed_resource::NONE)
            .manifest_required()
            .expect("无法嵌入 VoiceBar Windows 资源");
    }
}
