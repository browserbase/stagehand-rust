fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .type_attribute(".stagehand.v1.ModelConfiguration", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ModelObj", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.InitResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.LogLine", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.InitResult", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ActResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ExtractResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ObserveResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ExecuteRequest", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ExecuteResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        // Add Serialize/Deserialize for other request/response types if needed for JSON serialization in REST client
        .type_attribute(".stagehand.v1.ActRequest", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ExtractRequest", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.ObserveRequest", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.CloseRequest", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".stagehand.v1.CloseResponse", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile(&["proto/stagehand.v1.proto"], &["proto"])?;
    Ok(())
}