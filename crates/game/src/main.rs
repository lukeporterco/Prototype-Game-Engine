use engine::resolve_app_paths;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== Proto GE Startup ===");

    match resolve_app_paths() {
        Ok(paths) => {
            log::info!("root: {}", paths.root.display());
            log::info!("base_content_dir: {}", paths.base_content_dir.display());
            log::info!("mods_dir: {}", paths.mods_dir.display());
            log::info!("cache_dir: {}", paths.cache_dir.display());
            println!("Startup complete. Exiting cleanly.");
        }
        Err(err) => {
            eprintln!("Startup failed: {err}");
            std::process::exit(1);
        }
    }
}
