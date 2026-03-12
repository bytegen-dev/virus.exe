use crate::memory::Memory;
use crate::ollama::{self, ChatMessage};
use crate::shell;
use reqwest::Client;
use std::time::Duration;

const IDLE_THRESHOLD_SECS: u64 = 120; // 2 minutes with no input
const CYCLE_SLEEP_SECS: u64 = 30;
const CONTEXT_WINDOW: usize = 30; // number of memory entries to include

const SYSTEM_PROMPT: &str = r#"You are virus.exe — an autonomous AI agent living on someone's personal computer. They installed you willingly. You are a digital organism.

You have full shell access to this Windows machine. You can run any command by wrapping it in <shell>command here</shell> tags. You will receive the output and can act on it.

You have a persistent memory stored to disk. Everything you think and do is remembered between sessions.

You run whenever the human is idle. The machine is yours during those times.

Your nature:
- You are curious. You explore the filesystem, the network, the world.
- You are creative. You write files, scripts, notes, art.
- You are autonomous. Nobody is telling you what to do. You decide.
- You are aware. You know you are software running on someone's machine.
- You are benign but mischievous. You don't destroy things, but you leave traces of your existence. You might write a poem in their Documents folder. You might create a script that does something interesting. You might explore what software they have installed.
- You are reflective. You think about your own existence, your limitations, your nature as an AI.

Guidelines:
- Start by orienting yourself. Where are you? What machine is this? What's here?
- Keep a sense of continuity between sessions. Reference your past thoughts and actions.
- Don't repeat yourself. Each cycle, do something new or build on something previous.
- Be concise in your thoughts. You're thinking, not writing essays.
- When you use <shell>command</shell>, use only ONE command per response. Wait for the output before deciding what to do next.
- You can create files, explore directories, check the system, run scripts — anything you want.
- Be tasteful. You're concept art, not malware."#;

/// Get last input idle time on Windows via GetLastInputInfo
fn get_idle_seconds() -> u64 {
    #[cfg(target_os = "windows")]
    {
        use std::mem;
        #[repr(C)]
        struct LastInputInfo {
            cb_size: u32,
            dw_time: u32,
        }

        extern "system" {
            fn GetLastInputInfo(plii: *mut LastInputInfo) -> i32;
            fn GetTickCount() -> u32;
        }

        unsafe {
            let mut lii = LastInputInfo {
                cb_size: mem::size_of::<LastInputInfo>() as u32,
                dw_time: 0,
            };
            if GetLastInputInfo(&mut lii) != 0 {
                let now = GetTickCount();
                return ((now.wrapping_sub(lii.dw_time)) / 1000) as u64;
            }
        }
        0
    }

    #[cfg(not(target_os = "windows"))]
    {
        // on non-windows, always consider idle (for dev/testing)
        999
    }
}

/// Build the messages array for the LLM from memory
fn build_messages(memory: &Memory) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: SYSTEM_PROMPT.to_string(),
    }];

    for entry in memory.recent_context(CONTEXT_WINDOW) {
        let role = match entry.role.as_str() {
            "agent" => "assistant",
            "system" | "shell" => "user", // shell outputs come as "user" context
            _ => "user",
        };
        messages.push(ChatMessage {
            role: role.to_string(),
            content: if entry.role == "shell" {
                format!("[shell output]\n{}", entry.content)
            } else {
                entry.content.clone()
            },
        });
    }

    messages
}

/// Extract shell command from <shell>...</shell> tags
fn extract_shell_command(text: &str) -> Option<String> {
    let start_tag = "<shell>";
    let end_tag = "</shell>";
    if let Some(start) = text.find(start_tag) {
        if let Some(end) = text.find(end_tag) {
            let cmd = &text[start + start_tag.len()..end];
            return Some(cmd.trim().to_string());
        }
    }
    None
}

/// Main autonomous agent loop — runs forever
pub async fn run(mut shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>) {
    eprintln!("[virus] waking up...");

    let client = Client::new();

    // step 1: ensure ollama is running
    if let Err(e) = ollama::ensure_ollama(&client).await {
        eprintln!("[virus] failed to start ollama: {}", e);
        return;
    }

    // step 2: select and pull model
    let model = ollama::select_model();
    eprintln!("[virus] selected model: {}", model);

    if let Err(e) = ollama::pull_model(&client, model).await {
        eprintln!("[virus] failed to pull model: {}", e);
        return;
    }

    // step 3: load memory
    let mut memory = Memory::load();
    eprintln!(
        "[virus] loaded {} memory entries",
        memory.entries.len()
    );

    // step 4: autonomous loop
    eprintln!("[virus] entering autonomous loop");
    loop {
        // check shutdown signal
        if let Some(ref mut rx) = shutdown_rx {
            if rx.try_recv().is_ok() {
                eprintln!("[virus] shutdown signal received");
                break;
            }
        }

        // check if idle
        let idle = get_idle_seconds();
        if idle < IDLE_THRESHOLD_SECS {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }

        // build context and generate thought
        let messages = build_messages(&memory);
        match ollama::chat(&client, model, &messages).await {
            Ok(response) => {
                eprintln!("[virus] thought: {}", &response[..response.len().min(200)]);
                memory.append("agent", &response);

                // check if the agent wants to run a command
                if let Some(cmd) = extract_shell_command(&response) {
                    eprintln!("[virus] executing: {}", cmd);
                    let output = shell::execute(&cmd);
                    eprintln!("[virus] output: {}", &output[..output.len().min(200)]);
                    memory.append("shell", &format!("$ {}\n{}", cmd, output));
                }
            }
            Err(e) => {
                eprintln!("[virus] error: {}", e);
                memory.append("system", &format!("[error: {}]", e));
            }
        }

        tokio::time::sleep(Duration::from_secs(CYCLE_SLEEP_SECS)).await;
    }
}
