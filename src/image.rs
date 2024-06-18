use async_openai::{
    types::{CreateImageRequestArgs, ImageModel, ImageSize, ResponseFormat},
    Client,
};
use std::error::Error;
use tokio::time::{timeout, Duration};

pub async fn generate_and_save_image(prompt: &str) -> Result<(), Box<dyn Error>> {
    // Ensure the OpenAI API key is set in the environment variables
    let client = Client::new();

    // Create the image request
    let request = CreateImageRequestArgs::default()
        .prompt(prompt)
        .model(ImageModel::DallE3)
        .n(1)
        .response_format(ResponseFormat::Url)
        .size(ImageSize::S1024x1792) // High-res 2:3 aspect ratio
        .user("async-openai")
        .build()?;

    // Generate the image with a timeout
    let response = match timeout(Duration::from_secs(120), client.images().create(request)).await {
        Ok(res) => res?,
        Err(_) => {
            eprintln!("Error: The request timed out.");
            return Err("Request timed out.".into());
        }
    };

    // Log response details
    if response.data.is_empty() {
        eprintln!("Error: No image URLs received.");
        return Err("No image URLs received.".into());
    }

    // Save the generated image to the logs directory
    let paths = response.save("./data/logs").await?;

    paths
        .iter()
        .for_each(|path| println!("Image file path: {}", path.display()));

    Ok(())
}
