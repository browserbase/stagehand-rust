use stagehand::{Stagehand, V3Options, Env, Model, TransportChoice, AgentConfig, AgentExecuteOptions, ModelConfiguration};
use stagehand::{ActResponseEvent, ExtractResponseEvent, ExecuteResponseEvent};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct MovieInfo {
    title: String,
    rating: String,
    release_year: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load .env file
    dotenvy::dotenv().ok();

    // 1. Create client, specifying REST transport
    let mut stagehand = Stagehand::connect(
        TransportChoice::Rest("https://api.stagehand.browserbase.com/v1".to_string())
    ).await?;

    // 2. Configure V3 Options
    let opts = V3Options {
        env: Some(Env::Browserbase),
        verbose: Some(2),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        ..Default::default()
    };

    // 3. Init and capture session_id
    println!("Initializing...");
    stagehand.start(opts).await?;
    println!("Initialization Complete.");

    // 4. Act
    let mut act_stream = stagehand.act(
        "Go to imdb.com and search for 'The Matrix'",
        Some(Model::String("openai/gpt-5-nano".into())),
        HashMap::new(),
        Some(30_000),
        None,
    ).await?;

    while let Some(msg) = act_stream.next().await {
         if let Ok(event) = msg {
            match event.event {
                Some(ActResponseEvent::Log(log_msg)) => println!("[ACT LOG] {:?}", log_msg),
                Some(ActResponseEvent::Success(s)) => println!("[ACT RESULT] Success: {}", s),
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Act stream error: {:?}", e);
        }
    }

    // 5. Extract
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "rating": { "type": "string" },
            "release_year": { "type": "string" }
        }
    });

    let mut extract_stream = stagehand.extract(
        "Extract the top result movie info",
        schema,
        Some(Model::String("openai/gpt-5-nano".into())),
        None,
        None,
        None,
    ).await?;

    while let Some(msg) = extract_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(ExtractResponseEvent::Log(l)) => println!("[EXTRACT LOG] {:?}", l),
                Some(ExtractResponseEvent::DataJson(json)) => {
                    let movie: MovieInfo = serde_json::from_str(&json)?;
                    println!("[EXTRACT RESULT] Extracted Data: {:?}", movie);
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Extract stream error: {:?}", e);
        }
    }

    // 6. Execute with agent
    println!("Executing with agent...");
    let agent_config = AgentConfig {
        provider: None,
        model: Some(ModelConfiguration::String("openai/gpt-5-nano".into())),
        system_prompt: None,
        cua: None,
    };

    let execute_options = AgentExecuteOptions {
        instruction: "What is the URL of the current page?".to_string(),
        max_steps: Some(10),
        highlight_cursor: None,
    };

    let mut execute_stream = stagehand.execute(
        agent_config,
        execute_options,
        None,
    ).await?;

    while let Some(msg) = execute_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(ExecuteResponseEvent::Log(l)) => println!("[EXECUTE LOG] {:?}", l),
                Some(ExecuteResponseEvent::ResultJson(r)) => {
                    println!("[EXECUTE RESULT] {}", r);
                },
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Execute stream error: {:?}", e);
        }
    }

    // 7. Close
    stagehand.end().await?;

    Ok(())
}
