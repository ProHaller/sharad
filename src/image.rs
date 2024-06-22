use async_openai::{
    types::{CreateImageRequestArgs, ImageModel, ImageSize, ResponseFormat},
    Client,
};
use std::error::Error;
use tokio::time::{timeout, Duration};

pub async fn generate_and_save_image(prompt: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = Client::new();

    let request = CreateImageRequestArgs::default()
        .prompt(prompt)
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

    paths
        .iter()
        .for_each(|path| println!("Image file path: {}", path.display()));

    Ok(())
}

pub async fn generate_character_image(
    description: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = Client::new();

    let request = CreateImageRequestArgs::default()
        .prompt(description)
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
        Ok(path.display().to_string())
    } else {
        Err("No image file path received.".into())
    }
}
