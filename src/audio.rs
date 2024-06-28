use crate::display::Display;
use crate::error::SharadError;
use crate::Color;
use async_openai::error::OpenAIError;
use async_openai::{
    config::OpenAIConfig,
    types::{CreateSpeechRequestArgs, CreateTranscriptionRequestArgs, SpeechModel, Voice},
    Audio, Client,
};
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::event::{poll, read, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use std::{
    env,
    fs::{self},
    path::Path,
};
use tokio::task;

pub async fn generate_and_play_audio(
    audio: &Audio<'_, OpenAIConfig>,
    text: &str,
    role: &str,
) -> Result<(), SharadError> {
    let settings = crate::settings::load_settings()?;
    if !settings.audio_output_enabled {
        return Ok(());
    }

    let voice = match role {
        "Player" => Voice::Shimmer,
        "Game Master" => Voice::Onyx,
        _ => Voice::Onyx,
    };

    let response = audio
        .speech(
            CreateSpeechRequestArgs::default()
                .input(text)
                .voice(voice)
                .model(SpeechModel::Tts1)
                .speed(1.2)
                .build()
                .map_err(SharadError::OpenAI)?,
        )
        .await
        .map_err(SharadError::OpenAI)?;
    let file_name = format!("{}_{}.mp3", role, Local::now().format("%Y%m%d_%H%M%S"));
    let file_path = Path::new("./data/logs").join(file_name);
    fs::create_dir_all("./data/logs").map_err(SharadError::Io)?;
    response
        .save(file_path.to_str().unwrap())
        .await
        .map_err(SharadError::OpenAI)?;

    task::spawn_blocking(move || {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| SharadError::AudioPlaybackError(e.to_string()))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| SharadError::AudioPlaybackError(e.to_string()))?;
        let file = File::open(file_path).map_err(SharadError::Io)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| SharadError::AudioPlaybackError(e.to_string()))?;
        sink.append(source);
        sink.sleep_until_end();
        Ok::<(), SharadError>(())
    })
    .await
    .map_err(|e| SharadError::AudioPlaybackError(e.to_string()))??;

    Ok(())
}

pub async fn record_and_transcribe_audio(display: &mut Display) -> Result<String, SharadError> {
    let settings = crate::settings::load_settings()?;
    if !settings.audio_input_enabled {
        return Ok(String::new());
    }
    let recording_path = format!(
        "./data/logs/recording_{}.mp3",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );
    record_audio(&recording_path, display)?;

    let client = Client::with_config(OpenAIConfig::default().with_api_key(
        env::var("OPENAI_API_KEY").map_err(|_| SharadError::MissingAPIKey("OpenAI".into()))?,
    ));
    let audio = Audio::new(&client);

    println!();
    display.print_thinking_dot();

    match audio
        .transcribe(
            CreateTranscriptionRequestArgs::default()
                .file(&recording_path)
                .model("whisper-1")
                .build()
                .map_err(SharadError::OpenAI)?,
        )
        .await
    {
        Ok(transcription) => Ok(transcription.text),
        Err(e) => {
            if let OpenAIError::ApiError(api_err) = &e {
                if api_err.message.contains("Audio file is too short") {
                    if let Err(remove_err) = std::fs::remove_file(&recording_path) {
                        display.print_wrapped(
                            &format!("Failed to remove short audio file: {}", remove_err),
                            Color::Red,
                        );
                    }
                    return Ok(String::new());
                }
            }
            Err(SharadError::OpenAI(e))
        }
    }
}

fn record_audio(file_path: &str, display: &mut Display) -> Result<String, SharadError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| SharadError::AudioRecordingError("No input device available".into()))?;
    let config = device
        .default_input_config()
        .map_err(|e| SharadError::AudioRecordingError(e.to_string()))?;

    let spec = hound::WavSpec {
        channels: config.channels() as u16,
        sample_rate: config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(Some(
        hound::WavWriter::create(file_path, spec).map_err(SharadError::Hound)?,
    )));

    let is_recording = Arc::new(AtomicBool::new(false));
    let is_recording_clone = is_recording.clone();

    let stream = match config.sample_format() {
        cpal::SampleFormat::I16 => {
            let writer_clone = Arc::clone(&writer);
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        if let Some(guard) = writer_clone.lock().unwrap().as_mut() {
                            for &sample in data {
                                if let Err(e) = guard.write_sample(sample) {
                                    eprintln!("Error writing sample: {}", e);
                                }
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let writer_clone = Arc::clone(&writer);
            device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &_| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        if let Some(guard) = writer_clone.lock().unwrap().as_mut() {
                            for &sample in data {
                                let sample_i16 = sample.wrapping_sub(32768) as i16;
                                if let Err(e) = guard.write_sample(sample_i16) {
                                    eprintln!("Error writing sample: {}", e);
                                }
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::F32 => {
            let writer_clone = Arc::clone(&writer);
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        if let Some(guard) = writer_clone.lock().unwrap().as_mut() {
                            for &sample in data {
                                let sample_i16 = (sample * i16::MAX as f32) as i16;
                                if let Err(e) = guard.write_sample(sample_i16) {
                                    eprintln!("Error writing sample: {}", e);
                                }
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        _ => {
            return Err(SharadError::AudioRecordingError(
                "Unsupported sample format".into(),
            ))
        }
    }
    .map_err(|e| SharadError::AudioRecordingError(e.to_string()))?;

    stream
        .play()
        .map_err(|e| SharadError::AudioRecordingError(e.to_string()))?;

    let mut recording_start: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();
    let minimum_duration = Duration::from_secs(1); // Minimum 1 second recording

    enable_raw_mode().map_err(SharadError::Io)?;

    display.print_wrapped("Hold Space to record", Color::Yellow);

    loop {
        if poll(Duration::from_millis(10)).map_err(SharadError::Io)? {
            last_activity = Instant::now();
            if let Event::Key(key_event) = read().map_err(SharadError::Io)? {
                match key_event.code {
                    KeyCode::Char(' ') => {
                        if !is_recording.load(Ordering::Relaxed) {
                            is_recording.store(true, Ordering::Relaxed);
                            recording_start = Some(Instant::now());
                        }
                    }
                    KeyCode::Esc => break,
                    _ => {}
                }
            }
        } else if is_recording.load(Ordering::Relaxed)
            && last_activity.elapsed() > Duration::from_millis(300)
        {
            if let Some(start) = recording_start {
                let duration = start.elapsed();
                if duration >= minimum_duration {
                    is_recording.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
    }

    // Disable raw mode
    disable_raw_mode().map_err(SharadError::Io)?;

    // Ensure all data is written
    std::thread::sleep(Duration::from_millis(500));

    if let Some(guard) = writer.lock().unwrap().take() {
        guard.finalize().map_err(SharadError::Hound)?;
    }

    // Check if the recording meets the minimum duration
    if let Some(start) = recording_start {
        let duration = start.elapsed();
        if duration < minimum_duration {
            display.print_wrapped("Recording too short. Discarding.", Color::Red);
            std::fs::remove_file(file_path).map_err(SharadError::Io)?;
            Err(SharadError::AudioRecordingError(
                "Recording too short".into(),
            ))
        } else {
            display.print_wrapped("", Color::Green);
            Ok(file_path.to_string())
        }
    } else {
        Ok(String::new())
    }
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on stream: {}", err);
}
