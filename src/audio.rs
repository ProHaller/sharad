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
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use std::{
    env,
    error::Error,
    fs::{self},
    path::Path,
};
use tokio::task;

pub async fn generate_and_play_audio(
    audio: &Audio<'_, OpenAIConfig>,
    text: &str,
    role: &str,
) -> Result<(), Box<dyn Error>> {
    let settings = crate::settings::load_settings().unwrap_or_default();
    if !settings.audio_output_enabled {
        return Ok(());
    }

    let voice = match role {
        "user" => Voice::Shimmer,
        "narrator" => Voice::Nova,
        _ => Voice::Nova,
    };

    let response = audio
        .speech(
            CreateSpeechRequestArgs::default()
                .input(text)
                .voice(voice)
                .model(SpeechModel::Tts1)
                .speed(1.2)
                .build()?,
        )
        .await?;
    let file_name = format!("{}_{}.mp3", role, Local::now().format("%Y%m%d_%H%M%S"));
    let file_path = Path::new("./data/logs").join(file_name);
    fs::create_dir_all("./data/logs")?;
    response.save(file_path.to_str().unwrap()).await?;

    task::spawn_blocking(move || {
        let (_stream, stream_handle) =
            OutputStream::try_default().expect("Failed to get default output stream");
        let sink = Sink::try_new(&stream_handle).expect("Failed to create audio sink");
        sink.append(
            Decoder::new(BufReader::new(
                File::open(file_path).expect("Failed to open audio file"),
            ))
            .expect("Failed to decode audio file"),
        );
        sink.sleep_until_end();
    })
    .await?;

    Ok(())
}

pub async fn record_and_transcribe_audio() -> Result<String, Box<dyn Error>> {
    let recording_path = format!(
        "./data/logs/recording_{}.wav",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );
    record_audio(&recording_path)?;

    let client =
        Client::with_config(OpenAIConfig::default().with_api_key(env::var("OPENAI_API_KEY")?));
    let audio = Audio::new(&client);

    let transcription = audio
        .transcribe(
            CreateTranscriptionRequestArgs::default()
                .file(recording_path)
                .model("whisper-1")
                .build()?,
        )
        .await?;
    Ok(transcription.text)
}

fn record_audio(file_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("no input device available");
    let config = device
        .default_input_config()
        .expect("no default input config");

    let spec = hound::WavSpec {
        channels: config.channels() as u16,
        sample_rate: config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(Some(hound::WavWriter::create(file_path, spec)?)));

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
                                guard.write_sample(sample).unwrap();
                            }
                        }
                    }
                },
                err_fn,
                None,
            )?
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
                                guard.write_sample(sample_i16).unwrap();
                            }
                        }
                    }
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::F32 => {
            let writer_clone = Arc::clone(&writer);
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        if let Some(guard) = writer_clone.lock().unwrap().as_mut() {
                            for &sample in data {
                                guard
                                    .write_sample((sample * i16::MAX as f32) as i16)
                                    .unwrap();
                            }
                        }
                    }
                },
                err_fn,
                None,
            )?
        }
        _ => return Err("Unsupported sample format".into()),
    };

    stream.play()?;

    let mut recording_start: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();
    let minimum_duration = Duration::from_secs(1); // Minimum 1 second recording

    enable_raw_mode()?;

    println!("Hold Space to record");

    loop {
        if poll(Duration::from_millis(10))? {
            last_activity = Instant::now();
            if let Event::Key(key_event) = read()? {
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
        } else if is_recording.load(Ordering::Relaxed) {
            if last_activity.elapsed() > Duration::from_millis(300) {
                if let Some(start) = recording_start {
                    let duration = start.elapsed();
                    if duration >= minimum_duration {
                        is_recording.store(false, Ordering::Relaxed);
                        break;
                    } else {
                        std::io::stdout().flush()?;
                    }
                }
            }
        }
    }

    // Disable raw mode
    disable_raw_mode()?;

    // Ensure all data is written
    std::thread::sleep(Duration::from_millis(500));

    // Close the file
    if let Some(guard) = writer.lock().unwrap().take() {
        guard.finalize()?;
    }

    // Check if the recording meets the minimum duration
    if let Some(start) = recording_start {
        let duration = start.elapsed();
        if duration < minimum_duration {
            std::fs::remove_file(file_path)?; // Delete the file
            Ok(String::new()) // Return empty string
        } else {
            Ok(file_path.to_string()) // Return the file path
        }
    } else {
        Ok(String::new()) // Return empty string
    }
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on stream: {}", err);
}
