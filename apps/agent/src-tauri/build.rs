use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn file_digest(path: &Path) -> (u64, String) {
    let mut file = File::open(path).unwrap_or_else(|error| {
        panic!(
            "Required install asset {} is unavailable: {error}",
            path.display()
        )
    });
    let size = file.metadata().expect("asset metadata").len();
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer).expect("read install asset");
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    (size, format!("{:x}", hash.finalize()))
}

fn write_install_manifest() {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut source = String::from("const INSTALL_ASSETS: &[InstallAsset] = &[\n");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("PROFILE").as_deref() == Ok("release")
    {
        let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"))
            .join("../../../local_llm");
        let bin = root.join("bin");
        let mut names = fs::read_dir(&bin)
            .expect("local_llm/bin")
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let lower = name.to_ascii_lowercase();
                (lower.ends_with(".dll") || lower.ends_with(".exe")).then_some(name)
            })
            .collect::<Vec<_>>();
        names.sort();
        for required in [
            "llama-server.exe",
            "msvcp140.dll",
            "vcruntime140.dll",
            "vcruntime140_1.dll",
        ] {
            assert!(
                names.iter().any(|name| name.eq_ignore_ascii_case(required)),
                "{required} must be staged beside llama-server before release build"
            );
        }

        let mut assets = names
            .into_iter()
            .map(|name| (format!("local_llm/bin/{name}"), bin.join(name)))
            .collect::<Vec<_>>();
        for name in ["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"] {
            assets.push((name.to_string(), root.join("app_runtime").join(name)));
        }
        for name in ["APACHE-2.0.md", "MODEL_NOTICE.md"] {
            assets.push((format!("local_llm/{name}"), root.join(name)));
        }
        let browser_extension =
            PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"))
                .join("../browser-extension");
        for name in [
            "manifest.json",
            "options.html",
            "options.js",
            "worker.js",
            "music.js",
            "blocked.html",
            "blocked.js",
            "focus.css",
            "icon16.png",
            "icon48.png",
            "icon128.png",
        ] {
            assets.push((
                format!("browser-extension/{name}"),
                browser_extension.join(name),
            ));
        }

        for (relative, path) in assets {
            println!("cargo:rerun-if-changed={}", path.display());
            let (size, digest) = file_digest(&path);
            assert!(size > 0, "Install asset is empty: {relative}");
            source.push_str(&format!(
                "    InstallAsset {{ relative: {relative:?}, size: {size}, sha256: {digest:?} }},\n"
            ));
        }
    }

    source.push_str("];\n");
    File::create(out.join("install_manifest.rs"))
        .expect("create install manifest")
        .write_all(source.as_bytes())
        .expect("write install manifest");
}

fn main() {
    write_install_manifest();
    tauri_build::build();
}
