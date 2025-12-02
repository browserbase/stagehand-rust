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

#[tokio::test]
async fn test_browserbase_live_extract() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Set environment variables for API keys if using REST transport
    std::env::set_var("BROWSERBASE_API_KEY", "bb_live_qnzgygVsuPHJTiBkfPVxoUypSp8");
    std::env::set_var("BROWSERBASE_PROJECT_ID", "2b648897-a538-40b0-96d7-744ee6664341");
    std::env::set_var("OPENAI_API_KEY", "YOUR_OPENAI_API_KEY");

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

    // 3. Init with Progress Logging
    println!("Initializing...");
    let mut init_stream = stagehand.init(opts).await?;
    
    let mut session_id = String::new(); // Session ID is now handled internally by the transport

    while let Some(msg) = init_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(stagehand_sdk::proto::init_response::Event::Log(l)) => {
                    println!("[INIT LOG] {:?}", l);
                },
                Some(stagehand_sdk::proto::init_response::Event::Result(res)) => {
                    println!("Initialization Complete. Session ID (from event): {}", res.unused);
                    session_id = res.unused.clone(); // Still useful to have for execute
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Init stream error: {:?}", e);
            return Err(e.into());
        }
    }

    if session_id.is_empty() {
        // This check might be less reliable now, but we keep it for execute
        println!("Warning: Could not get session_id from init stream.");
    }

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
        session_id.clone(), // Execute still needs session_id passed in
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
