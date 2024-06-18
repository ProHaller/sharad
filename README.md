# Dynamic RPG Game with OpenAI GPT and Rust

## Overview

This project is an innovative role-playing game (RPG) developed in Rust, leveraging OpenAI's GPT for dynamic storytelling and world-building. The game combines the power of AI-generated narratives with structured game mechanics to create an engaging and immersive player experience.

## Features

- **Dynamic Storytelling**: AI-generated narrative elements that adapt to player choices.
- **Character Creation and Inventory Management**: Automated creation and management of game characters and items through function calling.
- **Interactive Gameplay**: Players directly interact with the AI to influence the game's storyline and events.

## Project Structure

- `src/main.rs`: The main entry point of the game.
- `src/api.rs`: Module for handling API requests and responses.
- `src/story_engine.rs`: Module for managing the narrative flow and game state.
- `src/function_calls.rs`: Module for handling function calls to update game state.
- `Cargo.toml`: Project configuration and dependencies.

## Getting Started

### Prerequisites

- Rust (latest stable version)
- OpenAI API Key

### Installation

1. **Clone the Repository**

   ```sh
   git clone https://github.com/yourusername/dynamic-rpg-game.git
   cd dynamic-rpg-game
   ```

2. **Set Up Environment Variables**
   Create a `.env` file in the project root and add your OpenAI API key:

   ```
   OPENAI_API_KEY=your_api_key_here
   ```

3. **Install Dependencies**

   ```sh
   cargo build
   ```

### Running the Game

To start the game, run:

```sh
cargo run
```
