use stagehand_sdk::{Stagehand, V3Options, Env, Model, Transport};
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
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // 1. Create client, specifying REST transport
    let mut stagehand = Stagehand::connect(
        "https://api.stagehand.browserbase.com/v1".to_string(), // REST API Base URL
        Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string())
    ).await?;

    // 2. Configure V3 Options
    let opts = V3Options {
        env: Some(Env::Browserbase), // Use Browserbase for REST API
        verbose: Some(2), // Detailed logging
        model: Some(Model::String("openai/gpt-4o".into())),
        // local_browser_launch_options is not relevant for Browserbase/REST
        ..Default::default()
    };

    // 3. Init with Progress Logging
    println!("Initializing...");
    let mut init_stream = stagehand.init(opts).await?;
    
    let mut session_id = String::new(); // To capture session_id from REST API Init

    while let Some(msg) = init_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::init_response::Event::Log(l)) => {
                    println!("[INIT LOG] {:?}", l);
                },
                Some(stagehand_sdk::proto::init_response::Event::Result(res)) => {
                    println!("Initialization Complete.");
                    if !res.unused.is_empty() {
                        session_id = res.unused.clone();
                    }
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Init stream error: {:?}", e);
            return Err(e.into());
        }
    }

    if session_id.is_empty() {
        panic!("Failed to initialize session");
    }

    // 4. Act (Note: Act, Extract, Observe will still show gRPC-style responses due to the current mapping in lib.rs)
    let mut act_stream = stagehand.act(
        "Go to imdb.com and search for 'The Matrix'",
        Some(Model::String("openai/gpt-4o".into())),
        HashMap::new(),
        Some(30_000),
        Some("main".to_string()),
    ).await?;

    // Wait for completion
    while let Some(msg) = act_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::act_response::Event::Log(log_msg)) => println!("[ACT LOG] {:?}", log_msg),
                Some(stagehand_sdk::proto::act_response::Event::Success(s)) => println!("[ACT RESULT] Success: {}", s),
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Act stream error: {:?}", e);
            return Err(e.into());
        }
    }

    // 5. Extract (Strongly Typed)
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
            return Err(e.into());
        }
    }

    // 6. Execute some JavaScript
    println!("Executing JavaScript...");
    let js_code = "return document.title;".to_string();
    let mut execute_stream = stagehand.execute(
        session_id.clone(),
        js_code,
    ).await?;

    let mut js_result: Option<String> = None;
    while let Some(msg) = execute_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::execute_response::Event::Progress(p)) => println!("[EXECUTE PROGRESS] {}", p),
                Some(stagehand_sdk::proto::execute_response::Event::Result(r)) => {
                    println!("[EXECUTE RESULT] {}", r);
                    js_result = Some(r);
                },
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Execute stream error: {:?}", e);
            return Err(e.into());
        }
    }
    assert!(js_result.is_some(), "Failed to execute JavaScript or get result.");
    println!("JavaScript execution result: {:?}", js_result.unwrap());

    // 7. Close
    stagehand.close(true).await?;
    
    Ok(())
}
