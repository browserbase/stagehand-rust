use stagehand_sdk::{Stagehand, V3Options, Env, Model, TransportChoice, AgentExecuteOptions};
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
        model: Some(Model::String("openai/gpt-4o".into())),
        ..Default::default()
    };

    // 3. Init and capture session_id
    println!("Initializing...");
    stagehand.init(opts).await?;
    println!("Initialization Complete.");

    // 4. Act
    let mut act_stream = stagehand.act(
        "Go to imdb.com and search for 'The Matrix'",
        Some(Model::String("openai/gpt-4o".into())),
        HashMap::new(),
        Some(30_000),
        Some("main".to_string()),
    ).await?;

    while let Some(msg) = act_stream.next().await {
         if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::act_response::Event::Log(log_msg)) => println!("[ACT LOG] {:?}", log_msg),
                Some(stagehand_sdk::proto::act_response::Event::Success(s)) => println!("[ACT RESULT] Success: {}", s),
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Act stream error: {:?}", e);
        }
    }

    // 5. Extract
    let schema_template = MovieInfo { 
        title: "".into(), rating: "".into(), release_year: "".into() 
    };

    let mut extract_stream = stagehand.extract(
        "Extract the top result movie info",
        &schema_template,
        Some(Model::String("openai/gpt-4o".into())),
        None,
        None,
        Some("main".to_string()),
    ).await?;

    while let Some(msg) = extract_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::extract_response::Event::Log(l)) => println!("[EXTRACT LOG] {:?}", l),
                Some(stagehand_sdk::proto::extract_response::Event::DataJson(json)) => {
                    let movie: MovieInfo = serde_json::from_str(&json)?;
                    println!("[EXTRACT RESULT] Extracted Data: {:?}", movie);
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Extract stream error: {:?}", e);
        }
    }
    
    // 6. Execute with agent-like signature
    println!("Executing with agent-like signature...");
    let agent_execute_options = AgentExecuteOptions {
        instruction: "What is the URL of the current page?".to_string(),
        page: Some("main".to_string()),
        timeout: Some(10_000),
    };

    let mut execute_stream = stagehand.execute(
        agent_execute_options.instruction.clone(),
        agent_execute_options.page.clone(),
        None,
        Some(agent_execute_options),
    ).await?;

    while let Some(msg) = execute_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::execute_response::Event::Progress(p)) => println!("[EXECUTE PROGRESS] {}", p),
                Some(stagehand_sdk::proto::execute_response::Event::ResultJson(r)) => {
                    println!("[EXECUTE RESULT] {}", r);
                },
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Execute stream error: {:?}", e);
        }
    }

    // 7. Close
    stagehand.close(true).await?;
    
    Ok(())
}