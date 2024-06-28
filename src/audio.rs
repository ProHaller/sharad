use crate::display::Display;
use crate::error::SharadError;
use async_openai::error::OpenAIError;
use async_openai::{
    config::OpenAIConfig,
    types::{CreateSpeechRequestArgs, CreateTranscriptionRequestArgs, SpeechModel, Voice},
    Audio, Client,
};
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::event::{self, Event, KeyCode};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use hound::WavWriter;
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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

pub async fn record_and_transcribe_audio(display: &mut Display) -> Result<String, Box<dyn Error>> {
    let settings = crate::settings::load_settings().unwrap_or_default();
    if !settings.audio_input_enabled {
        return Ok(String::new());
    }
    let recording_path = format!(
        "./data/logs/recording_{}.mp3",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );
    record_audio(&recording_path, display)?;

    let client =
        Client::with_config(OpenAIConfig::default().with_api_key(env::var("OPENAI_API_KEY")?));
    let audio = Audio::new(&client);

    println!();
    display.print_wrapped("Transcribing audio", Color::Yellow);
    display.print_thinking_dot();

    match audio
        .transcribe(
            CreateTranscriptionRequestArgs::default()
                .file(&recording_path)
                .model("whisper-1")
                .build()?,
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
            Err(e.into())
        }
    }
}

pub fn record_audio(file_path: &str, display: &mut Display) -> Result<String, SharadError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| SharadError::AudioRecordingError("No input device available".into()))?;
    let config = device.default_input_config().map_err(|e| {
        SharadError::AudioRecordingError(format!("Failed to get default input config: {}", e))
    })?;

    let spec = hound::WavSpec {
        channels: config.channels() as u16,
        sample_rate: config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(Some(
        WavWriter::create(file_path, spec).map_err(|e| {
            SharadError::AudioRecordingError(format!("Failed to create WAV writer: {}", e))
        })?,
    )));

    let is_recording = Arc::new(AtomicBool::new(false));
    let is_recording_clone = is_recording.clone();

    let stream = match config.sample_format() {
        cpal::SampleFormat::I16 => build_stream::<i16>(
            &device,
            &config.into(),
            writer.clone(),
            is_recording_clone.clone(),
        )?,
        cpal::SampleFormat::U16 => build_stream::<i16>(
            &device,
            &config.into(),
            writer.clone(),
            is_recording_clone.clone(),
        )?,
        cpal::SampleFormat::F32 => build_stream::<f32>(
            &device,
            &config.into(),
            writer.clone(),
            is_recording_clone.clone(),
        )?,
        _ => {
            return Err(SharadError::AudioRecordingError(
                "Unsupported sample format".into(),
            ))
        }
    };

    stream
        .play()
        .map_err(|e| SharadError::AudioRecordingError(format!("Failed to play stream: {}", e)))?;

    let mut recording_start: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();
    let minimum_duration = Duration::from_secs(1); // Minimum 1 second recording

    enable_raw_mode().map_err(|e| {
        SharadError::AudioRecordingError(format!("Failed to enable raw mode: {}", e))
    })?;

    display.print_wrapped("Hold Space to record", Color::Yellow);

    loop {
        if event::poll(Duration::from_millis(10)).map_err(|e| {
            SharadError::AudioRecordingError(format!("Failed to poll for event: {}", e))
        })? {
            last_activity = Instant::now();
            if let Event::Key(key_event) = event::read().map_err(|e| {
                SharadError::AudioRecordingError(format!("Failed to read event: {}", e))
            })? {
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

    disable_raw_mode().map_err(|e| {
        SharadError::AudioRecordingError(format!("Failed to disable raw mode: {}", e))
    })?;

    // Ensure all data is written
    std::thread::sleep(Duration::from_millis(500));

    // Close the file
    if let Some(guard) = writer.lock().unwrap().take() {
        guard.finalize().map_err(|e| {
            SharadError::AudioRecordingError(format!("Failed to finalize WAV writer: {}", e))
        })?;
    }

    // Check if the recording meets the minimum duration
    if let Some(start) = recording_start {
        let duration = start.elapsed();
        if duration < minimum_duration {
            display.print_wrapped("Recording too short. Discarding.", Color::Red);
            std::fs::remove_file(file_path).map_err(|e| {
                SharadError::AudioRecordingError(format!(
                    "Failed to remove short audio file: {}",
                    e
                ))
            })?;
            Ok(String::new())
        } else {
            display.print_wrapped("", Color::Green);
            Ok(file_path.to_string())
        }
    } else {
        Ok(String::new())
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    writer: Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>,
    is_recording: Arc<AtomicBool>,
) -> Result<cpal::Stream, SharadError>
where
    T: cpal::Sample + hound::Sample + cpal::SizedSample,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &_| {
                if is_recording.load(Ordering::Relaxed) {
                    if let Some(guard) = writer.lock().unwrap().as_mut() {
                        for &sample in data {
                            let sample = sample.as_i16();
                            if let Err(e) = guard.write_sample(sample) {
                                eprintln!("Error writing sample: {}", e);
                                break;
                            }
                        }
                    }
                }
            },
            |err| eprintln!("An error occurred on the audio stream: {}", err),
            None,
        )
        .map_err(|e| {
            SharadError::AudioRecordingError(format!("Failed to build input stream: {}", e))
        })
}
