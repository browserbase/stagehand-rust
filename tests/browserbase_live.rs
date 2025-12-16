use stagehand_sdk::{Stagehand, V3Options, Env, Model, TransportChoice, AgentConfig, AgentExecuteOptions, ModelConfiguration};
use stagehand_sdk::{ActResponseEvent, ExtractResponseEvent, ExecuteResponseEvent, NavigateResponseEvent, ObserveResponseEvent};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct PageInfo {
    title: String,
    description: String,
}

#[tokio::test]
async fn test_browserbase_live() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables from .env
    dotenvy::dotenv().ok();

    // 1. Create client, specifying REST transport (uses STAGEHAND_API_URL env var or default)
    let mut stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;

    // 2. Configure V3 Options
    let opts = V3Options {
        env: Some(Env::Browserbase),
        verbose: Some(2),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        ..Default::default()
    };

    // 3. Start session
    println!("Starting session...");
    stagehand.start(opts).await?;
    println!("Session ID: {:?}", stagehand.session_id());

    // 4. Navigate to example.com
    println!("\n=== NAVIGATE ===");
    let mut nav_stream = stagehand.navigate("https://example.com", Some(30_000), None).await?;

    while let Some(msg) = nav_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(NavigateResponseEvent::Log(log_msg)) => println!("[NAV LOG] {:?}", log_msg),
                Some(NavigateResponseEvent::Success(s)) => println!("[NAV RESULT] Success: {}", s),
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Navigate stream error: {:?}", e);
        }
    }

    // 5. Observe - find elements on the page
    println!("\n=== OBSERVE ===");
    let mut observe_stream = stagehand.observe(
        Some("Find the main heading and any links on the page".to_string()),
        Some(Model::String("openai/gpt-5-nano".into())),
        Some(30_000),
        None,
        None,
    ).await?;

    while let Some(msg) = observe_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(ObserveResponseEvent::Log(l)) => println!("[OBSERVE LOG] {:?}", l),
                Some(ObserveResponseEvent::ElementsJson(json)) => {
                    println!("[OBSERVE RESULT] Elements: {}", json);
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Observe stream error: {:?}", e);
        }
    }

    // 6. Extract - get page info
    println!("\n=== EXTRACT ===");
    // Schema must be in JSON Schema format, not a template object
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" }
        }
    });

    let mut extract_stream = stagehand.extract(
        "Extract the page title and description text",
        schema,
        Some(Model::String("openai/gpt-5-nano".into())),
        Some(30_000),
        None,
        None,
    ).await?;

    while let Some(msg) = extract_stream.next().await {
        if let Ok(event) = msg {
            match event.event {
                Some(ExtractResponseEvent::Log(l)) => println!("[EXTRACT LOG] {:?}", l),
                Some(ExtractResponseEvent::DataJson(json)) => {
                    if json == "null" || json.is_empty() {
                        println!("[EXTRACT RESULT] No data extracted");
                    } else {
                        match serde_json::from_str::<PageInfo>(&json) {
                            Ok(info) => println!("[EXTRACT RESULT] Page Info: {:?}", info),
                            Err(e) => println!("[EXTRACT RESULT] Parse error: {} - Raw: {}", e, json),
                        }
                    }
                }
                _ => {}
            }
        } else if let Err(e) = msg {
            eprintln!("Extract stream error: {:?}", e);
        }
    }

    // 7. Act - click on the "More information" link
    println!("\n=== ACT ===");
    let mut act_stream = stagehand.act(
        "Click on the 'More information...' link",
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

    // 8. Execute with agent - verify where we are
    println!("\n=== EXECUTE ===");
    let agent_config = AgentConfig {
        provider: None,
        model: Some(ModelConfiguration::String("openai/gpt-5-nano".into())),
        system_prompt: None,
        cua: None,
    };

    let execute_options = AgentExecuteOptions {
        instruction: "What is the current page URL and title?".to_string(),
        max_steps: Some(5),
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

    // 9. Close
    println!("\n=== CLOSE ===");
    stagehand.end().await?;
    println!("Session closed successfully");

    Ok(())
}
