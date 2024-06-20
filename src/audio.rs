use async_openai::{
    config::OpenAIConfig,
    types::{CreateSpeechRequestArgs, CreateTranscriptionRequestArgs, SpeechModel, Voice},
    Audio, Client,
};
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::{Decoder, OutputStream, Sink};
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{stdin, BufReader},
    path::Path,
    sync::{Arc, Mutex},
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

fn record_audio(file_path: &str) -> Result<(), Box<dyn Error>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("Failed to get default input device");
    let config = device
        .default_input_config()
        .expect("Failed to get default input config");

    let spec = hound::WavSpec {
        channels: config.channels() as u16,
        sample_rate: config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(hound::WavWriter::create(file_path, spec)?));
    let writer_clone = writer.clone();

    println!("{:^80}", "Press Enter to start recording...");
    let mut input = String::new();
    stdin().read_line(&mut input)?;

    println!("{:^80}", "Recording... Press Enter to stop.");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                let mut writer = writer.lock().unwrap();
                for &sample in data.iter() {
                    writer
                        .write_sample((sample * i16::MAX as f32) as i16)
                        .unwrap();
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                let mut writer = writer_clone.lock().unwrap();
                for &sample in data.iter() {
                    writer.write_sample(sample).unwrap();
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &_| {
                let mut writer = writer_clone.lock().unwrap();
                for &sample in data.iter() {
                    writer.write_sample(sample as i16 - i16::MAX).unwrap();
                }
            },
            err_fn,
            None,
        )?,
        _ => todo!(),
    };

    stream.play()?;

    stdin().read_line(&mut input)?;
    drop(stream);
    println!("{:^80}", "Recording stopped.");
    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on stream: {}", err);
}
