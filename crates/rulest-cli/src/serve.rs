use std::path::Path;

pub fn run_with_options(db_path: &str, auto_validate: bool) -> Result<(), String> {
    let path = Path::new(db_path);
    if !path.exists() {
        return Err(format!(
            "Registry not found at {}. Run `rulest init` first.",
            db_path
        ));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

    rt.block_on(rulest_mcp::server::run_stdio_with_options(path, auto_validate))
}
