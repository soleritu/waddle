//! Failures in plain words: what happened, why, and what to do. Every error the dashboard shows
//! passes through here, so a raw "500" or "os error 112" never reaches the person on its own.

use crate::config::re;
use crate::util::Ctx;
use std::fs;

/// The end of a log, for the reason behind a failure.
fn log_tail(ctx: &Ctx, name: &str) -> String {
    fs::read(ctx.logs().join(name)).map(|b| String::from_utf8_lossy(&b[b.len().saturating_sub(12_000)..]).into_owned()).unwrap_or_default()
}

/// Which other programs hold video memory right now, from the latest reading.
pub fn vram_holders(ctx: &Ctx) -> String {
    let _ = ctx;
    let holders: Vec<String> = crate::monitor::last_sample()
        .and_then(|s| s["procs"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|p| p["vram"].as_f64().unwrap_or(0.0) >= 0.5)
        .map(|p| format!("{} (process {}) holds {:.1} GB", p["name"].as_str().unwrap_or("a program"), p["pid"], p["vram"].as_f64().unwrap_or(0.0)))
        .collect();
    if holders.is_empty() { String::new() } else { format!(" Right now {}.", holders.join("; ")) }
}

fn free_disk(ctx: &Ctx) -> String {
    crate::system::free_space(&ctx.root).map(|b| format!(" {:.1} GB is free.", b / 1e9)).unwrap_or_default()
}

/// The plain-words version of an error. Errors already written for people pass through unchanged.
pub fn explain(ctx: &Ctx, raw: &str) -> String {
    let text = raw.trim();
    let has = |pattern: &'static str| re(pattern).is_match(text);
    // messages Waddle already wrote end with a sentence and name a next step; keep them
    let model_log = || log_tail(ctx, "router.log");

    if has(r"(?i)exceeds? the available context|context (length|size) (exceeded|is too small)|n_ctx|prompt is too long|too many tokens") {
        return "The conversation is longer than the model's context window, so it can't read it all. Compact the chat, start a new one, or raise ctx-size in the model's settings (it uses more memory).".into();
    }
    if has(r"(?i)has wrong type .* but expected type|unknown model architecture|unknown projector type|invalid magic|wrong number of tensors") {
        return "This file was converted for a different llama.cpp, or for a fork of it, so this llama.cpp can't read it. Get the model from ggml-org or the official conversion instead, or update llama.cpp in Settings › Updates.".into();
    }
    if has(r"(?i)loading model|503") && has(r"(?i)load") {
        return "The model is still loading into memory. Try again in a few seconds.".into();
    }
    if has(r"(?i)model.*not found|unknown model|no such model|model .* does not exist") {
        return "There's no model by that name. Check the exact name under My models or Connect apps; names are case-sensitive.".into();
    }
    if has(r"(?i)(image|multimodal|vision).*(not supported|isn't supported|unsupported)|(image|vision).{0,40}mmproj|mmproj.{0,40}(image|vision)") && !has(r"(?i)llama-tts|text to speak|tts") {
        return "This model can't read images. Turn on Vision in its panel (it needs a vision projector, mmproj), or pick a model tagged vision.".into();
    }
    if has(r"(?i)audio.*(not supported|unsupported)") {
        return "This model can't hear audio. Use an audio model, such as Voxtral or Qwen2.5-Omni, with its audio projector.".into();
    }
    if has(r"(?i)tools? (param|parameter)?.*(requires|not supported)|does not support tool|jinja") {
        return "This model's chat template doesn't support tool calls. Pick a model tagged tools, such as Qwen3 or Llama 3.1+, for Tools mode.".into();
    }
    if has(r"(?i)grammar|json.?schema") && has(r"(?i)fail|invalid|error|parse") {
        return "The output format couldn't be used: the JSON schema or grammar is invalid. Check it for typos, or try Free text.".into();
    }
    if has(r"(?i)failed to load|loading.*failed|model.*crash|exited|500") && !has(r"(?i)image server") {
        let log = model_log();
        if re(r"(?i)out of (device )?memory|ErrorOutOfDeviceMemory|failed to allocate|cudaMalloc|vk::.*OutOfDeviceMemory").is_match(&log) {
            return format!("The model didn't fit in video memory.{} Lower its context (ctx-size), turn on a q8_0 cache, use a smaller quantization, or run Tune in its Performance tab to find settings that fit.", vram_holders(ctx));
        }
        if re(r"(?i)unknown model architecture|unsupported model|invalid magic|failed to read magic").is_match(&log) {
            return "llama.cpp doesn't support this model file: it may be too new for this build, or not a GGUF model. Get the newest llama.cpp in Settings, or pick another file.".into();
        }
        if re(r"(?i)no such file|cannot open|failed to open").is_match(&log) {
            return "A file the model needs is missing. Check its files in the model's panel, or download it again.".into();
        }
        if has(r"(?i)failed to load") {
            return "The model failed to load. The reason is at the end of Logs › Model server.".into();
        }
    }
    if has(r"(?i)isn't answering|connection refused|actively refused|ConnectionRefused|10061") {
        return "The model server isn't running, so nothing answered. Start it on the Overview, or see Logs › Model server for why it stopped.".into();
    }
    if has(r"(?i)timed out|timeout|took (longer|too long)") {
        return "It took too long and was stopped. A large model or a long context can be slow here; try a smaller one, or a shorter request.".into();
    }
    if has(r"(?i)\bdns\b|could not resolve|failed to resolve|no such host|network is unreachable|failed to lookup|host not found") {
        return "Waddle can't reach the internet, so Hugging Face and GitHub aren't available. Check your connection; everything already downloaded still works.".into();
    }
    if has(r"(?i)\b(401|403)\b|gated|unauthorized|forbidden") && has(r"(?i)huggingface|hf|download|resolve") {
        return "Hugging Face refused the download. This model is gated: open its page on huggingface.co and accept the license, then add a read token under Settings › Hugging Face. Or choose an ungated copy, such as one from unsloth or bartowski.".into();
    }
    if has(r"(?i)\b429\b|too many requests|rate.?limit") && has(r"(?i)huggingface|hf|download|resolve") {
        return "Hugging Face is slowing down requests from your address. Add a free read token under Settings › Hugging Face, or wait a few minutes and press Resume.".into();
    }
    if has(r"(?i)\b404\b") && has(r"(?i)huggingface|resolve") {
        return "That file is no longer on Hugging Face; the repo may have been renamed or the file removed. Search for the model again.".into();
    }
    if has(r"(?i)os error 112|no space left|not enough space|disk full|ENOSPC") {
        return format!("The drive is full.{} Delete unused files under Settings › Unused files, or models you don't use.", free_disk(ctx));
    }
    if has(r"(?i)os error 5\b|access is denied|permission denied|EACCES") {
        return "Windows refused access to a file. Another program may have it open, or antivirus may be holding it; close the other program, or try again in a moment.".into();
    }
    if has(r"(?i)os error (32|33)|being used by another process") {
        return "A file is in use by another program. Close it, or stop the model or image server using it, and try again.".into();
    }
    if has(r"(?i)address already in use|os error 10048|only one usage of each socket") {
        return "Another program is already using that port. Close it, or change the port in Settings › Model server.".into();
    }
    if has(r"(?i)^The server said 5\d\d:?\s*$") || text.is_empty() {
        return "Something went wrong on the server's side, without a reason. See Logs for details.".into();
    }
    text.to_string()
}
