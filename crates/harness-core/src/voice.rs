//! Speech — microphone recording, speech-to-text and text-to-speech.
//!
//! Ported from Aura Code's `src/tools/dictate.ts` (STT + TTS) so the desktop
//! harness can do the same thing without a terminal:
//!
//!   * **STT chain**: Xiaomi MiMo ASR → Groq Whisper → OpenAI Whisper →
//!     ElevenLabs. The first configured provider that answers wins.
//!   * **TTS chain**: Xiaomi MiMo TTS → ElevenLabs → OpenAI. MiMo is the same
//!     voice engine Aura Code uses (`mimo-v2.5-tts`).
//!   * **Recording** uses `pw-record` (PipeWire, handles Bluetooth headsets),
//!     then `parec`, then `arecord`, always as raw PCM converted to 16 kHz mono
//!     WAV with `ffmpeg` when recording stops.
//!
//! Keys come from the same universal lookup as the rest of the app:
//! `~/.aura/keys.json`, `~/.aura/.env`, process env, or the harness
//! credentials file.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use base64::Engine;
use serde_json::{json, Value};

use crate::config::load_api_key_named;
use crate::error::{Error, Result};

const SAMPLE_RATE: u32 = 16_000;

/// One synthesised clip, with the file extension its container needs.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

// ─────────────────────────────────────────────────────────────────────────────
// Availability
// ─────────────────────────────────────────────────────────────────────────────

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let path = dir.join(bin);
                path.is_file()
            })
        })
        .unwrap_or(false)
}

pub fn recorder_available() -> bool {
    which("pw-record") || which("parec") || which("arecord")
}

pub fn converter_available() -> bool {
    which("ffmpeg")
}

pub fn player_available() -> bool {
    which("ffplay") || which("paplay") || which("pw-play") || which("aplay")
}

/// Providers that will be tried for speech-to-text, in order.
pub fn stt_providers() -> Vec<&'static str> {
    let mut out = Vec::new();
    if std::env::var("PARAKEET_BASE_URL").ok().filter(|v| !v.trim().is_empty()).is_some() {
        out.push("Parakeet (local)");
    }
    if has_key(&["XIAOMI_API_KEY"]) {
        out.push("Xiaomi MiMo ASR");
    }
    if has_key(&["GROQ_API_KEY"]) {
        out.push("Groq Whisper");
    }
    if has_key(&["OPENAI_API_KEY"]) {
        out.push("OpenAI Whisper");
    }
    if has_key(&["ELEVENLABS_API_KEY"]) {
        out.push("ElevenLabs");
    }
    out
}

/// Providers that will be tried for text-to-speech, in order.
pub fn tts_providers() -> Vec<&'static str> {
    let mut out = Vec::new();
    if has_key(&["XIAOMI_API_KEY"]) {
        out.push("Xiaomi MiMo TTS");
    }
    if has_key(&["ELEVENLABS_API_KEY"]) {
        out.push("ElevenLabs");
    }
    if has_key(&["OPENAI_API_KEY"]) {
        out.push("OpenAI TTS");
    }
    out
}

fn has_key(names: &[&str]) -> bool {
    names.iter().any(|n| load_api_key_named(n).is_some())
}

// ─────────────────────────────────────────────────────────────────────────────
// Recording
// ─────────────────────────────────────────────────────────────────────────────

/// A live microphone recording. Dropping it without [`Recorder::stop`] kills
/// the child process and leaves the raw file behind in the temp directory.
pub struct Recorder {
    child: Child,
    raw: PathBuf,
    started: Instant,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Closing the app mid-recording must not leave pw-record behind.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Recorder {
    /// Start recording from the default source.
    pub fn start() -> Result<Self> {
        if !recorder_available() {
            return Err(Error::Invalid(
                "no recorder found — install pw-record, parec or arecord".into(),
            ));
        }
        let raw = std::env::temp_dir().join(format!(
            "aura-harness-voice-{}.raw",
            uuid::Uuid::new_v4()
        ));

        let child = if which("pw-record") {
            Command::new("pw-record")
                .arg("--rate")
                .arg(SAMPLE_RATE.to_string())
                .arg("--format")
                .arg("s16")
                .arg("--channels")
                .arg("1")
                .arg("--raw")
                .arg(&raw)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        } else if which("parec") {
            let file = std::fs::File::create(&raw)?;
            Command::new("parec")
                .arg("--rate")
                .arg(SAMPLE_RATE.to_string())
                .arg("--format")
                .arg("s16le")
                .arg("--channels")
                .arg("1")
                .arg("--raw")
                .stdin(Stdio::null())
                .stdout(Stdio::from(file))
                .stderr(Stdio::null())
                .spawn()?
        } else {
            let file = std::fs::File::create(&raw)?;
            Command::new("arecord")
                .arg("-r")
                .arg(SAMPLE_RATE.to_string())
                .arg("-f")
                .arg("S16_LE")
                .arg("-c")
                .arg("1")
                .arg("-t")
                .arg("raw")
                .stdin(Stdio::null())
                .stdout(Stdio::from(file))
                .stderr(Stdio::null())
                .spawn()?
        };

        Ok(Self { child, raw, started: Instant::now() })
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// Stop recording and return a 16 kHz mono WAV path.
    pub fn stop(mut self) -> Result<PathBuf> {
        // std's kill is SIGKILL; the recorder has already flushed everything
        // written so far, and a raw stream has no header to finalise.
        let _ = self.child.kill();
        let _ = self.child.wait();

        if !self.raw.exists() {
            return Err(Error::Invalid("the recorder wrote no audio".into()));
        }
        let wav = self.raw.with_extension("wav");
        if which("ffmpeg") {
            let status = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "s16le",
                    "-ar",
                    &SAMPLE_RATE.to_string(),
                    "-ac",
                    "1",
                    "-i",
                ])
                .arg(&self.raw)
                .arg(&wav)
                .status()?;
            let _ = std::fs::remove_file(&self.raw);
            if !status.success() || !wav.exists() {
                return Err(Error::Invalid("could not convert the recording to WAV".into()));
            }
            Ok(wav)
        } else {
            // Without ffmpeg, hand back the raw stream renamed .wav: some
            // providers accept headerless PCM as audio/wav, and the error is
            // clearer than "ffmpeg missing" from deep inside the network code.
            let fallback = self.raw.with_extension("wav");
            std::fs::rename(&self.raw, &fallback)?;
            Ok(fallback)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Speech to text
// ─────────────────────────────────────────────────────────────────────────────

/// Transcribe a WAV file through the provider chain.
pub async fn transcribe(http: &reqwest::Client, audio: &Path) -> Result<String> {
    let providers = stt_providers();
    if providers.is_empty() {
        return Err(Error::Invalid(
            "no speech-to-text key set — add XIAOMI_API_KEY, GROQ_API_KEY, OPENAI_API_KEY \
             or ELEVENLABS_API_KEY to ~/.aura/.env"
                .into(),
        ));
    }
    let bytes = tokio::fs::read(audio).await?;
    if bytes.len() < 512 {
        return Err(Error::Invalid("recording is too short to transcribe".into()));
    }

    let mut errors = Vec::new();
    for provider in providers {
        let result = match provider {
            "Parakeet (local)" => {
                let base = std::env::var("PARAKEET_BASE_URL").unwrap_or_default();
                let key = std::env::var("PARAKEET_API_KEY").unwrap_or_else(|_| "sk-local".into());
                transcribe_whisper(
                    http,
                    base.trim_end_matches('/'),
                    &key,
                    "parakeet-tdt-0.6b",
                    &bytes,
                )
                .await
            }
            "Xiaomi MiMo ASR" => transcribe_mimo(http, &bytes).await,
            "Groq Whisper" => {
                let key = load_api_key_named("GROQ_API_KEY")
                    .ok_or_else(|| Error::Invalid("GROQ_API_KEY is not set".into()))?;
                transcribe_whisper(
                    http,
                    "https://api.groq.com/openai/v1",
                    &key,
                    "whisper-large-v3-turbo",
                    &bytes,
                )
                .await
            }
            "OpenAI Whisper" => {
                let key = load_api_key_named("OPENAI_API_KEY")
                    .ok_or_else(|| Error::Invalid("OPENAI_API_KEY is not set".into()))?;
                transcribe_whisper(
                    http,
                    "https://api.openai.com/v1",
                    &key,
                    "whisper-1",
                    &bytes,
                )
                .await
            }
            "ElevenLabs" => transcribe_elevenlabs(http, &bytes).await,
            _ => continue,
        };
        match result {
            Ok(text) if !text.trim().is_empty() => return Ok(text.trim().to_string()),
            Ok(_) => errors.push(format!("{provider}: no speech detected")),
            Err(e) => errors.push(format!("{provider}: {e}")),
        }
    }
    Err(Error::Invalid(format!(
        "transcription failed — {}",
        errors.join("; ")
    )))
}

async fn transcribe_mimo(http: &reqwest::Client, bytes: &[u8]) -> Result<String> {
    if bytes.len() > 25_000_000 {
        return Err(Error::Invalid("recording is larger than the 25 MB ASR limit".into()));
    }
    let key = load_api_key_named("XIAOMI_API_KEY")
        .ok_or_else(|| Error::Invalid("XIAOMI_API_KEY is not set".into()))?;
    let base = xiaomi_base_url(&key);
    let audio = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    let body = json!({
        "model": "mimo-v2.5-asr",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "input_audio",
                "input_audio": { "data": audio }
            }]
        }],
        "asr_options": { "language": "auto" }
    });
    let resp = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    let value: Value = resp.json().await?;
    Ok(value["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

async fn transcribe_whisper(
    http: &reqwest::Client,
    base: &str,
    key: &str,
    model: &str,
    bytes: &[u8],
) -> Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav")?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string())
        .text("response_format", "text".to_string());
    let resp = http
        .post(format!("{base}/audio/transcriptions"))
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    let text = resp.text().await?;
    // response_format=text answers plain text, but some gateways still wrap it.
    if text.trim_start().starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(t) = value.get("text").and_then(Value::as_str) {
                return Ok(t.to_string());
            }
        }
    }
    Ok(text)
}

async fn transcribe_elevenlabs(http: &reqwest::Client, bytes: &[u8]) -> Result<String> {
    let key = load_api_key_named("ELEVENLABS_API_KEY")
        .ok_or_else(|| Error::Invalid("ELEVENLABS_API_KEY is not set".into()))?;
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav")?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model_id", "scribe_v1".to_string());
    let resp = http
        .post("https://api.elevenlabs.io/v1/speech-to-text")
        .header("xi-api-key", key)
        .multipart(form)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    let value: Value = resp.json().await?;
    Ok(value.get("text").and_then(Value::as_str).unwrap_or_default().to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Text to speech
// ─────────────────────────────────────────────────────────────────────────────

/// Synthesize `text` with the first configured TTS provider.
pub async fn synthesize(http: &reqwest::Client, text: &str, voice: &str) -> Result<AudioClip> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::Invalid("nothing to speak".into()));
    }
    let providers = tts_providers();
    if providers.is_empty() {
        return Err(Error::Invalid(
            "no text-to-speech key set — add XIAOMI_API_KEY, ELEVENLABS_API_KEY \
             or OPENAI_API_KEY to ~/.aura/.env"
                .into(),
        ));
    }
    let mut errors = Vec::new();
    for provider in providers {
        let result = match provider {
            "Xiaomi MiMo TTS" => tts_mimo(http, text, voice).await,
            "ElevenLabs" => tts_elevenlabs(http, text, voice).await,
            "OpenAI TTS" => tts_openai(http, text, voice).await,
            _ => continue,
        };
        match result {
            Ok(clip) => return Ok(clip),
            Err(e) => errors.push(format!("{provider}: {e}")),
        }
    }
    Err(Error::Invalid(format!("text-to-speech failed — {}", errors.join("; "))))
}

fn xiaomi_base_url(key: &str) -> String {
    let from_env = std::env::var("XIAOMI_BASE_URL").ok();
    xiaomi_base_url_with(key, from_env.as_deref())
}

fn xiaomi_base_url_with(key: &str, override_url: Option<&str>) -> String {
    if let Some(url) = override_url {
        let url = url.trim().trim_end_matches('/');
        if !url.is_empty() {
            return url.to_string();
        }
    }
    if key.starts_with("tp-") {
        "https://token-plan-sgp.xiaomimimo.com/v1".into()
    } else {
        "https://api.xiaomimimo.com/v1".into()
    }
}

async fn tts_mimo(http: &reqwest::Client, text: &str, voice: &str) -> Result<AudioClip> {
    let key = load_api_key_named("XIAOMI_API_KEY")
        .ok_or_else(|| Error::Invalid("XIAOMI_API_KEY is not set".into()))?;
    let base = xiaomi_base_url(&key);
    let selected = if voice.trim().is_empty() { "mimo_default" } else { voice.trim() };
    let body = json!({
        "model": "mimo-v2.5-tts",
        "messages": [
            { "role": "user", "content": "Read the following text clearly and naturally with appropriate expression." },
            { "role": "assistant", "content": text }
        ],
        "audio": { "format": "wav", "voice": selected }
    });
    let resp = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    let value: Value = resp.json().await?;
    let encoded = value["choices"][0]["message"]["audio"]["data"]
        .as_str()
        .ok_or_else(|| Error::Invalid("MiMo returned no audio data".into()))?;
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| Error::Invalid(format!("bad base64 audio: {e}")))?;
    // MiMo occasionally prefixes the RIFF WAV with CRLF.
    if bytes.len() >= 2 && bytes[0] == 0x0d && bytes[1] == 0x0a {
        bytes.drain(..2);
    }
    Ok(AudioClip { bytes, extension: "wav" })
}

async fn tts_elevenlabs(http: &reqwest::Client, text: &str, voice: &str) -> Result<AudioClip> {
    let key = load_api_key_named("ELEVENLABS_API_KEY")
        .ok_or_else(|| Error::Invalid("ELEVENLABS_API_KEY is not set".into()))?;
    // A configured ElevenLabs voice id is a long opaque token. The MiMo
    // defaults (mimo_default, Mia, …) must not be sent as ElevenLabs ids.
    let voice_id = if voice.len() >= 15 && !voice.starts_with("mimo_") {
        voice
    } else {
        "21m00Tcm4TlvDq8ikWAM" // Rachel
    };
    let body = json!({
        "text": text,
        "model_id": "eleven_multilingual_v2"
    });
    let resp = http
        .post(format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}"))
        .header("xi-api-key", key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    Ok(AudioClip { bytes: resp.bytes().await?.to_vec(), extension: "mp3" })
}

async fn tts_openai(http: &reqwest::Client, text: &str, voice: &str) -> Result<AudioClip> {
    let key = load_api_key_named("OPENAI_API_KEY")
        .ok_or_else(|| Error::Invalid("OPENAI_API_KEY is not set".into()))?;
    let allowed = ["alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse"];
    let voice = if allowed.contains(&voice) { voice } else { "alloy" };
    let body = json!({
        "model": "gpt-4o-mini-tts",
        "voice": voice,
        "input": text,
        "response_format": "mp3"
    });
    let resp = http
        .post("https://api.openai.com/v1/audio/speech")
        .bearer_auth(key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }
    Ok(AudioClip { bytes: resp.bytes().await?.to_vec(), extension: "mp3" })
}

// ─────────────────────────────────────────────────────────────────────────────
// Playback
// ─────────────────────────────────────────────────────────────────────────────

/// Write a clip to the temp directory and play it. Blocking: callers should
/// run it on a worker thread.
pub fn play_clip(clip: &AudioClip) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "aura-harness-speech-{}.{}",
        uuid::Uuid::new_v4(),
        clip.extension
    ));
    let mut file = std::fs::File::create(&path)?;
    file.write_all(&clip.bytes)?;
    file.flush()?;
    play_file(&path)?;
    Ok(path)
}

/// Play an audio file with whatever player this machine has.
pub fn play_file(path: &Path) -> Result<()> {
    let status = if which("ffplay") {
        Command::new("ffplay")
            .arg("-nodisp")
            .arg("-autoexit")
            .arg("-loglevel")
            .arg("quiet")
            .arg(path)
            .status()?
    } else if which("paplay") {
        Command::new("paplay").arg(path).status()?
    } else if which("pw-play") {
        Command::new("pw-play").arg(path).status()?
    } else if which("aplay") {
        Command::new("aplay").arg(path).status()?
    } else {
        return Err(Error::Invalid(
            "no audio player found — install ffplay, paplay, pw-play or aplay".into(),
        ));
    };
    if !status.success() {
        return Err(Error::Invalid("the audio player exited with an error".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_lists_reflect_keys_without_panicking() {
        // Environment-dependent by design; the invariant is that every name is
        // one of the providers the chains implement.
        for name in stt_providers() {
            assert!(["Parakeet (local)", "Xiaomi MiMo ASR", "Groq Whisper", "OpenAI Whisper", "ElevenLabs"].contains(&name));
        }
        for name in tts_providers() {
            assert!(["Xiaomi MiMo TTS", "ElevenLabs", "OpenAI TTS"].contains(&name));
        }
    }

    #[test]
    fn xiaomi_endpoint_follows_key_type() {
        assert!(xiaomi_base_url_with("sk-test", None).contains("api.xiaomimimo.com"));
        assert!(xiaomi_base_url_with("tp-test", None).contains("token-plan-sgp"));
        assert_eq!(
            xiaomi_base_url_with("tp-test", Some("https://example.test/v1")),
            "https://example.test/v1"
        );
    }

    #[test]
    fn recording_tools_are_detected() {
        // These are present on Aura OS; on a bare CI box the assertion is only
        // that the probe itself works.
        let _ = recorder_available();
        let _ = player_available();
        let _ = converter_available();
    }

    /// Live round trip: MiMo TTS → WAV → MiMo ASR. No playback, so it is safe
    /// to run on the user's machine. Run with:
    /// `cargo test -p harness-core voice::tests::live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_tts_stt_round_trip() {
        let http = reqwest::Client::new();
        let spoken = "Aura harness voice test. The microphone path is ready.";
        let clip = synthesize(&http, spoken, "mimo_default")
            .await
            .expect("text to speech");
        assert!(!clip.bytes.is_empty(), "TTS returned no audio");

        let path = std::env::temp_dir().join(format!(
            "aura-harness-voice-test-{}.{}",
            uuid::Uuid::new_v4(),
            clip.extension
        ));
        std::fs::write(&path, &clip.bytes).unwrap();
        if clip.extension != "wav" {
            let wav = path.with_extension("wav");
            let status = Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(&path)
                .arg(&wav)
                .status()
                .expect("ffmpeg");
            assert!(status.success());
            std::fs::remove_file(&path).ok();
            let text = transcribe(&http, &wav).await.expect("speech to text");
            std::fs::remove_file(&wav).ok();
            assert!(!text.trim().is_empty());
        } else {
            let text = transcribe(&http, &path).await.expect("speech to text");
            std::fs::remove_file(&path).ok();
            println!("round trip heard: {text}");
            assert!(text.to_lowercase().contains("voice") || text.to_lowercase().contains("aura"));
        }
    }
}
