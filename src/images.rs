//! Pictures made with stable-diffusion.cpp, kept in images/ with what they were made from.

use crate::config::re;
use crate::util::{now, Ctx, Res};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub fn folder(ctx: &Ctx) -> PathBuf {
    ctx.root.join("images")
}

/// A picture's file. Names come from the page, so they're checked before they touch the disk.
pub fn file(ctx: &Ctx, name: &str) -> Res<PathBuf> {
    if !re(r"^[\w-]{1,120}\.png$").is_match(name) {
        return Err("That isn't a picture Waddle made.".into());
    }
    Ok(folder(ctx).join(name))
}

/// A file name from the time and the prompt's first words, such as 20261122-0945-a-penguin-on-ice.png.
pub fn file_name(prompt: &str, stamp: &str) -> String {
    let words: String = prompt
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let words: String = words.chars().take(48).collect();
    format!("{stamp}-{}.png", if words.is_empty() { "image" } else { &words })
}

pub fn decode_base64(text: &str) -> Res<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c).ok_or("The picture came back damaged.")? << (18 - 6 * i);
        }
        out.extend(&n.to_be_bytes()[1..chunk.len()]);
    }
    Ok(out)
}

/// Saves a picture and a small note of how it was made beside it.
pub fn save(ctx: &Ctx, png: &[u8], prompt: &str, settings: &Value, seconds: f64) -> Res<Value> {
    fs::create_dir_all(folder(ctx)).map_err(|e| e.to_string())?;
    let stamp = time_stamp();
    let mut name = file_name(prompt, &stamp);
    let mut extra = 2;
    while folder(ctx).join(&name).exists() {
        name = file_name(prompt, &format!("{stamp}-{extra}"));
        extra += 1;
    }
    fs::write(folder(ctx).join(&name), png).map_err(|e| e.to_string())?;
    let note = json!({"name": name, "prompt": prompt, "settings": settings, "seconds": (seconds * 10.0).round() / 10.0, "t": now()});
    fs::write(folder(ctx).join(name.replace(".png", ".json")), note.to_string()).map_err(|e| e.to_string())?;
    Ok(note)
}

/// Every picture, newest first, with its prompt when the note is there.
pub fn list(ctx: &Ctx) -> Value {
    let mut found: Vec<Value> = fs::read_dir(folder(ctx))
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.ends_with(".png") || file(ctx, &name).is_err() {
                        return None;
                    }
                    let note = fs::read_to_string(folder(ctx).join(name.replace(".png", ".json"))).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
                    let t = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64());
                    Some(note.unwrap_or_else(|| json!({"name": name, "prompt": "", "t": t})))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by(|a, b| b["t"].as_f64().unwrap_or(0.0).total_cmp(&a["t"].as_f64().unwrap_or(0.0)));
    json!({"images": found})
}

pub fn delete(ctx: &Ctx, name: &str) -> Res<Value> {
    let path = file(ctx, name)?;
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(path.with_extension("json"));
    Ok(json!({}))
}

/// The time as yyyymmdd-hhmmss in UTC, without a date library. Names only need to sort and not collide.
pub fn time_stamp() -> String {
    let seconds = now() as i64;
    // Civil date from days since 1970 (Howard Hinnant's algorithm), in UTC.
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}
