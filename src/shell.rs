use std::process::Command;

const MAX_OUTPUT: usize = 4096;

/// Execute a shell command via cmd.exe and return combined stdout+stderr.
/// Output is truncated to MAX_OUTPUT bytes.
pub fn execute(cmd: &str) -> String {
    let result = Command::new("cmd")
        .args(["/C", cmd])
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut combined = format!("{}{}", stdout, stderr);
            if combined.len() > MAX_OUTPUT {
                combined.truncate(MAX_OUTPUT);
                combined.push_str("\n... [truncated]");
            }
            if combined.is_empty() {
                format!("[exit code: {}]", output.status.code().unwrap_or(-1))
            } else {
                combined
            }
        }
        Err(e) => format!("[error: {}]", e),
    }
}
