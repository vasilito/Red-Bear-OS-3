use std::env;
use std::path::Path;

fn main() {
    let mut args: Vec<String> = env::args().collect();
    // Ensure all flags go to cargo
    if args.len() >= 2 {
        args.insert(2, "--".to_string());
        if args[1] == "write-exec" {
            if let Ok(stage_dir) = std::env::var("COOKBOOK_STAGE") {
                let folder = format!("{stage_dir}/root");

                args.insert(2, stage_dir);
                args.insert(2, "--root".to_string());

                if Path::new(&folder).exists() {
                    args.insert(2, folder);
                    args.insert(2, "--folder".to_string());
                }
            }
        }
    }
    redoxer::main(&args);
}
