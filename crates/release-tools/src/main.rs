use base64::{engine::general_purpose::STANDARD, Engine};
use minisign::SecretKeyBox;
use std::{env, fs, io::Cursor, path::Path};

fn main() {
    if let Err(error) = run() {
        eprintln!("Signing failed: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("expected installer-path and version".into());
    }
    let path = Path::new(&args[0]);
    let version = semver::Version::parse(args[1].trim_start_matches('v'))
        .map_err(|_| "invalid release version")?;
    let private = env::var("TAURI_SIGNING_PRIVATE_KEY")
        .map_err(|_| "TAURI_SIGNING_PRIVATE_KEY is missing")?;
    let private = if private.trim_start().starts_with("untrusted comment:") {
        private
    } else {
        String::from_utf8(
            STANDARD
                .decode(private.trim())
                .map_err(|_| "invalid private key encoding")?,
        )
        .map_err(|_| "invalid private key encoding")?
    };
    let secret = SecretKeyBox::from_string(&private)
        .map_err(|_| "invalid private key")?
        .into_secret_key(Some(
            env::var("TAURI_SIGNING_PRIVATE_KEY_PASSWORD").unwrap_or_default(),
        ))
        .map_err(|_| "cannot decrypt the private key; check its password")?;
    let bytes = fs::read(path).map_err(|_| "cannot read installer")?;
    let signature = minisign::sign(
        None,
        &secret,
        Cursor::new(bytes),
        Some(&format!("version:{version}")),
        Some("VoiceBar release"),
    )
    .map_err(|_| "cannot sign installer")?;
    fs::write(
        format!("{}.sig", path.display()),
        STANDARD.encode(signature.to_string().as_bytes()),
    )
    .map_err(|_| "cannot write installer signature")?;
    println!("Signed installer version {version}");
    Ok(())
}
