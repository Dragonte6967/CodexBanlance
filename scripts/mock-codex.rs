//! Minimal test-only JSONL server. Compile with rustc, never distribute.
use std::{io::{self, BufRead, Write}, thread, time::Duration};
fn main() {
    let mode = std::env::var("WIDGET_FIXTURE_MODE").unwrap_or_default();
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let Some(tail) = line.split("\"id\":").nth(1) else { continue; };
        let id: String = tail.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
        if id.is_empty() { continue; }
        let result = if line.contains("\"initialize\"") { "{\"userAgent\":\"fixture\"}" }
        else if line.contains("\"account/read\"") {
            if mode == "logged-out" { "{\"account\":null}" } else { "{\"account\":{\"type\":\"chatgpt\"}}" }
        } else if line.contains("\"account/rateLimits/read\"") {
            match mode.as_str() {
                "timeout" => { thread::sleep(Duration::from_secs(120)); continue; }
                "crash" => std::process::exit(3),
                "offline" => {
                    println!("{{\"id\":{id},\"error\":{{\"code\":-1,\"message\":\"network unavailable\"}}}}");
                    io::stdout().flush().unwrap(); continue;
                }
                _ => "{\"rateLimitsByLimitId\":{\"codex\":{\"primary\":{\"usedPercent\":25,\"windowDurationMins\":300,\"resetsAt\":2000000000},\"secondary\":{\"usedPercent\":100,\"windowDurationMins\":10080,\"resetsAt\":2000000000}},\"other\":{\"limitName\":\"另一额度\",\"primary\":{\"usedPercent\":40,\"windowDurationMins\":60}}}}"
            }
        } else { "{}" };
        println!("{{\"id\":{id},\"result\":{result}}}");
        io::stdout().flush().unwrap();
    }
}
