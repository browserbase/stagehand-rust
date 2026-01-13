use stagehand_sdk::{
    ActResponseEvent, AgentConfig, AgentExecuteOptions, Env, ExecuteResponseEvent,
    ExtractResponseEvent, Model, ModelConfiguration, ObserveResponseEvent, Stagehand,
    TransportChoice, V3Options,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct Comment {
    text: String,
    author: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    println!("=== Stagehand Rust SDK Example ===\n");

    // Environment variables required:
    // - BROWSERBASE_API_KEY
    // - BROWSERBASE_PROJECT_ID
    // - A model API key (OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_GENERATIVE_AI_API_KEY, etc.)

    // 1. Initialize client with API keys (from environment variables)
    println!("1. Connecting to Stagehand...");
    let mut stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;
    println!("   Connected!\n");

    // 2. Start session with model_name (NO deprecated headers)
    println!("2. Starting browser session...");
    let opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        verbose: Some(2),
        ..Default::default()
    };

    stagehand.start(opts).await?;
    println!("   Session ID: {:?}\n", stagehand.session_id());

    // 3. Navigate to https://news.ycombinator.com
    println!("3. Navigating to Hacker News...");
    let mut act_stream = stagehand
        .act(
            "Navigate to https://news.ycombinator.com",
            None,
            HashMap::new(),
            Some(60_000),
            None,
        )
        .await?;

    while let Some(res) = act_stream.next().await {
        if let Ok(response) = res {
            if let Some(ActResponseEvent::Success(success)) = response.event {
                println!("   Navigation success: {}\n", success);
            }
        }
    }

    // 4. Observe to find "link to view comments for the top post"
    println!("4. Finding link to view comments for the top post...");
    let mut observe_stream = stagehand
        .observe(
            Some("Find the link to view comments for the top post".to_string()),
            None,
            Some(60_000),
            None,
            None,
        )
        .await?;

    let mut elements_json = String::new();
    while let Some(res) = observe_stream.next().await {
        if let Ok(response) = res {
            if let Some(ObserveResponseEvent::ElementsJson(json)) = response.event {
                elements_json = json;
                println!("   Found elements!\n");
            }
        }
    }

    // 5. Act on the first action from observe results
    println!("5. Clicking on the comments link...");
    let mut act_stream = stagehand
        .act(
            "Click on the comments link for the top post",
            None,
            HashMap::new(),
            Some(60_000),
            None,
        )
        .await?;

    while let Some(res) = act_stream.next().await {
        if let Ok(response) = res {
            if let Some(ActResponseEvent::Success(success)) = response.event {
                println!("   Click success: {}\n", success);
            }
        }
    }

    // 6. Extract top comment text + author using JSON schema
    println!("6. Extracting top comment and author...");
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "The text content of the top comment" },
            "author": { "type": "string", "description": "The username of the comment author" }
        },
        "required": ["text", "author"]
    });

    let mut extract_stream = stagehand
        .extract(
            "Extract the text and author of the top comment on the page",
            schema,
            None,
            Some(60_000),
            None,
            None,
        )
        .await?;

    let mut comment_data = String::new();
    while let Some(res) = extract_stream.next().await {
        if let Ok(response) = res {
            if let Some(ExtractResponseEvent::DataJson(json)) = response.event {
                comment_data = json.clone();
                if let Ok(comment) = serde_json::from_str::<Comment>(&json) {
                    println!("   Top comment by {}: {}\n", comment.author, comment.text);
                }
            }
        }
    }

    // 7. Execute autonomous agent to find author's profile (GitHub/LinkedIn/website)
    println!("7. Finding author's profile using autonomous agent...");
    let agent_config = AgentConfig {
        provider: None,
        model: Some(ModelConfiguration::String("openai/gpt-5-nano".into())),
        system_prompt: None,
        cua: None,
    };

    let comment: Comment = serde_json::from_str(&comment_data)?;
    let execute_options = AgentExecuteOptions {
        instruction: format!(
            "Find the profile page for the Hacker News user '{}'. Look for links to their GitHub, LinkedIn, or personal website.",
            comment.author
        ),
        max_steps: Some(10),
        highlight_cursor: None,
    };

    let mut execute_stream = stagehand
        .execute(agent_config, execute_options, None)
        .await?;

    while let Some(res) = execute_stream.next().await {
        if let Ok(response) = res {
            match response.event {
                Some(ExecuteResponseEvent::Log(log)) => {
                    println!("   [Agent Log] {:?}", log);
                }
                Some(ExecuteResponseEvent::ResultJson(result)) => {
                    println!("   Agent result: {}\n", result);
                }
                _ => {}
            }
        }
    }

    // 8. End session
    println!("8. Closing session...");
    stagehand.end().await?;
    println!("   Session closed successfully!\n");

    println!("=== Example completed! ===");

    Ok(())
}
