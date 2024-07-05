use crate::display::Display;
use crate::error::SharadError;
use crate::utils::open_image;
use crate::Color;
use async_openai::{
    types::{CreateImageRequestArgs, ImageModel, ImageSize, ResponseFormat},
    Client,
};
use std::error::Error;
use tokio::time::{timeout, Duration};

pub async fn generate_and_save_image(prompt: &str) -> Result<(), SharadError> {
    let client = Client::new();
    let mut display = Display::new();

    let request = CreateImageRequestArgs::default()
        .prompt(prompt)
        .model(ImageModel::DallE3)
        .n(1)
        .response_format(ResponseFormat::Url)
        .size(ImageSize::S1024x1792)
        .user("async-openai")
        .build()
        .map_err(|e| SharadError::Other(e.to_string()))?;

    let response = match timeout(Duration::from_secs(120), client.images().create(request)).await {
        Ok(res) => res.map_err(SharadError::OpenAI)?,
        Err(_) => return Err(SharadError::Other("Request timed out.".into())),
    };

    if response.data.is_empty() {
        return Err(SharadError::Other("No image URLs received.".into()));
    }

    let paths = response
        .save("./data/logs")
        .await
        .map_err(SharadError::OpenAI)?;

    paths.iter().for_each(|path| {
        display.print_debug(
            &format!("Image file path: {}", path.display()),
            Color::Magenta,
        )
    });

    if let Some(path) = paths.first() {
        handle_generated_image(path.to_str().unwrap(), &mut display).await?;
        Ok(())
    } else {
        Err(SharadError::Other("No image file path received.".into()))
    }
}

pub async fn handle_generated_image(
    image_path: &str,
    display: &mut Display,
) -> Result<(), SharadError> {
    display.print_debug(&format!("Image generated: {}", image_path), Color::Magenta);

    match open_image(image_path) {
        Ok(_) => {
            display.print_wrapped("Image opened successfully.", Color::Green);
            Ok(())
        }
        Err(e) => {
            display.print_wrapped(&format!("Failed to open image: {}", e), Color::Yellow);
            Ok(())
        }
    }
}

pub async fn generate_character_image(
    character_info: CharacterInfo,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = Client::new();

    let prompt = build_image_prompt(&character_info);

    let request = CreateImageRequestArgs::default()
        .prompt(&prompt)
        .model(ImageModel::DallE3)
        .n(1)
        .response_format(ResponseFormat::Url)
        .size(ImageSize::S1024x1792)
        .user("async-openai")
        .build()?;

    let response = match timeout(Duration::from_secs(120), client.images().create(request)).await {
        Ok(res) => res?,
        Err(_) => {
            eprintln!("Error: The request timed out.");
            return Err("Request timed out.".into());
        }
    };

    if response.data.is_empty() {
        eprintln!("Error: No image URLs received.");
        return Err("No image URLs received.".into());
    }

    let paths = response.save("./data/logs").await?;

    if let Some(path) = paths.first() {
        let _ = handle_generated_image(path.to_str().unwrap(), &mut Display::new()).await;
        Ok(path.display().to_string())
    } else {
        Err("No image file path received.".into())
    }
}

pub struct CharacterInfo {
    pub name: String,
    pub appearance: Appearance,
    pub distinctive_signs: Vec<String>,
    pub accessories: Vec<String>,
    pub location: String,
    pub ambiance: String,
    pub environment: String,
    pub image_generation_prompt: String,
}

pub struct Appearance {
    pub gender: String,
    pub age: String,
    pub height: String,
    pub build: String,
    pub hair: String,
    pub eyes: String,
    pub skin: String,
}

fn build_image_prompt(character_info: &CharacterInfo) -> String {
    let mut prompt = format!(
        "Generate an image of {}, a {} {} with {} build, {} tall. ",
        character_info.name,
        character_info.appearance.age,
        character_info.appearance.gender,
        character_info.appearance.build,
        character_info.appearance.height
    );

    prompt += &format!(
        "They have {} hair, {} eyes, and {} skin. ",
        character_info.appearance.hair,
        character_info.appearance.eyes,
        character_info.appearance.skin
    );

    if !character_info.distinctive_signs.is_empty() {
        prompt += &format!(
            "Distinctive features: {}. ",
            character_info.distinctive_signs.join(", ")
        );
    }

    if !character_info.accessories.is_empty() {
        prompt += &format!("Wearing: {}. ", character_info.accessories.join(", "));
    }

    prompt += &format!(
        "The character is located in {}, with a {} ambiance. The surrounding environment is {}. ",
        character_info.location, character_info.ambiance, character_info.environment
    );

    prompt += &character_info.image_generation_prompt;

    let mut display = Display::new();
    display.print_debug(&prompt, Color::Magenta);
    prompt
}
