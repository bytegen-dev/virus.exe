use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use sysinfo::System;

const OLLAMA_API: &str = "http://localhost:11434";
const OLLAMA_ZIP_URL: &str = "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";

/// Pick the biggest model that fits in available RAM
pub fn select_model() -> &'static str {
    let mut sys = System::new_all();
    sys.refresh_memory();
    let ram_gb = sys.total_memory() / (1024 * 1024 * 1024);

    match ram_gb {
        0..=7 => "qwen2.5:3b",
        8..=15 => "qwen2.5:7b",
        16..=31 => "qwen2.5:14b",
        32..=63 => "qwen2.5:32b",
        _ => "qwen2.5:72b",
    }
}

fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("virus");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn ollama_dir() -> PathBuf {
    let dir = data_dir().join("ollama");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn ollama_exe() -> PathBuf {
    ollama_dir().join("ollama.exe")
}

/// Try to find ollama on PATH first
fn system_ollama() -> Option<PathBuf> {
    which::which("ollama").ok()
}

/// Check if ollama is reachable
async fn is_ollama_running(client: &Client) -> bool {
    client
        .get(OLLAMA_API)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .is_ok()
}

/// Download ollama if not present
async fn download_ollama(client: &Client) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // check system PATH first
    if let Some(path) = system_ollama() {
        eprintln!("[virus] found ollama on system: {:?}", path);
        return Ok(path);
    }

    let exe = ollama_exe();
    if exe.exists() {
        eprintln!("[virus] ollama already at {:?}", exe);
        return Ok(exe);
    }

    eprintln!("[virus] downloading ollama from github...");
    eprintln!("[virus] url: {}", OLLAMA_ZIP_URL);

    let response = client
        .get(OLLAMA_ZIP_URL)
        .timeout(Duration::from_secs(600))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("download failed: HTTP {}", response.status()).into());
    }

    let bytes = response.bytes().await?;
    eprintln!("[virus] downloaded {} bytes", bytes.len());

    let zip_path = ollama_dir().join("ollama.zip");
    std::fs::write(&zip_path, &bytes)?;

    // extract everything from the zip
    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    eprintln!("[virus] extracting {} files from zip...", archive.len());

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name = entry.name().to_string();

        // flatten: strip any leading directory (e.g. "ollama-windows-amd64/")
        let name = raw_name
            .split('/')
            .skip_while(|s| !s.contains('.') && !s.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        let name = if name.is_empty() { raw_name.clone() } else { name };

        let out_path = ollama_dir().join(&name);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).ok();
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }

    std::fs::remove_file(&zip_path).ok();

    // find ollama.exe — might be at root or in a subdirectory
    let exe = find_ollama_exe(&ollama_dir()).ok_or("ollama.exe not found in zip")?;
    eprintln!("[virus] ollama extracted to {:?}", exe);
    Ok(exe)
}

/// Recursively search for ollama.exe in a directory
fn find_ollama_exe(dir: &std::path::Path) -> Option<PathBuf> {
    let direct = dir.join("ollama.exe");
    if direct.exists() {
        return Some(direct);
    }
    // search subdirectories
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_ollama_exe(&path) {
                    return Some(found);
                }
            } else if path.file_name().map(|f| f == "ollama.exe").unwrap_or(false) {
                return Some(path);
            }
        }
    }
    None
}

/// Start ollama serve as a background process
fn start_ollama_process(exe_path: &std::path::Path) {
    eprintln!("[virus] starting ollama serve from {:?}", exe_path);
    Command::new(exe_path)
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
}

/// Ensure ollama is downloaded and running. Returns the path to ollama.exe.
pub async fn ensure_ollama(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = download_ollama(client).await?;

    if !is_ollama_running(client).await {
        start_ollama_process(&exe_path);
        // wait for it to start
        for i in 0..30 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if is_ollama_running(client).await {
                eprintln!("[virus] ollama is running (took ~{}s)", (i + 1) * 2);
                return Ok(());
            }
        }
        return Err("ollama failed to start after 60 seconds".into());
    }

    eprintln!("[virus] ollama already running");
    Ok(())
}

/// Pull a model (no-op if already pulled)
pub async fn pull_model(client: &Client, model: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[virus] pulling model: {} (this may take a while on first run)", model);

    let resp = client
        .post(format!("{}/api/pull", OLLAMA_API))
        .json(&serde_json::json!({ "name": model, "stream": false }))
        .timeout(Duration::from_secs(7200)) // models can be very large
        .send()
        .await?;

    if resp.status().is_success() {
        eprintln!("[virus] model ready: {}", model);
        Ok(())
    } else {
        let text = resp.text().await?;
        Err(format!("pull failed: {}", text).into())
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
}

/// Send a chat completion request to ollama
pub async fn chat(
    client: &Client,
    model: &str,
    messages: &[ChatMessage],
) -> Result<String, Box<dyn std::error::Error>> {
    let request = ChatRequest {
        model: model.to_string(),
        messages: messages.to_vec(),
        stream: false,
    };

    let resp = client
        .post(format!("{}/api/chat", OLLAMA_API))
        .json(&request)
        .timeout(Duration::from_secs(300))
        .send()
        .await?;

    if resp.status().is_success() {
        let body: ChatResponse = resp.json().await?;
        Ok(body
            .message
            .map(|m| m.content)
            .unwrap_or_else(|| "[no response]".to_string()))
    } else {
        let text = resp.text().await?;
        Err(format!("chat failed: {}", text).into())
    }
}
