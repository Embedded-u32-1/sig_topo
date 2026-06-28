use signal_topology::export::to_dot;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: stv <topology.json>");
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let input_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("topology");
    let parent = input_path.parent().unwrap_or_else(|| Path::new("."));

    let json = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to read '{}': {}", input_path.display(), e);
        std::process::exit(1);
    });

    let schema: signal_topology::schema::TopologySchema = serde_json::from_str(&json)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse topology JSON: {}", e);
            std::process::exit(1);
        });

    let dot = to_dot(&schema);
    let dot_path = parent.join(format!("{}.dot", input_stem));
    fs::write(&dot_path, dot).unwrap_or_else(|e| {
        eprintln!("Failed to write '{}': {}", dot_path.display(), e);
        std::process::exit(1);
    });
    println!("Generated {}", dot_path.display());

    let svg_path = parent.join(format!("{}.svg", input_stem));
    match Command::new("dot").arg("-V").output() {
        Ok(_) => {
            match Command::new("dot")
                .arg("-Tsvg")
                .arg(&dot_path)
                .arg("-o")
                .arg(&svg_path)
                .status()
            {
                Ok(status) if status.success() => {
                    println!("Generated {}", svg_path.display());
                }
                Ok(status) => {
                    eprintln!(
                        "'dot' exited with status {}. SVG was not generated.",
                        status
                    );
                }
                Err(e) => {
                    eprintln!("Failed to run 'dot': {}", e);
                }
            }
        }
        Err(_) => {
            println!(
                "Graphviz 'dot' not found in PATH. Install Graphviz to generate '{}'.",
                svg_path.display()
            );
        }
    }
}
