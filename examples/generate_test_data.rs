use rand::Rng;
use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let num_files: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let messages_per_file: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let output_dir = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "./test-data-large".to_string());

    println!("Generating {} files with {} messages each...", num_files, messages_per_file);
    println!("Output directory: {}", output_dir);
    println!("Total messages: {}", num_files * messages_per_file);

    // Create output directory
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let mut rng = rand::thread_rng();

    for file_num in 0..num_files {
        let filename = format!("{}/trades_{:05}.xml", output_dir, file_num);
        let mut file = fs::File::create(&filename).expect("Failed to create file");

        // Write XML header
        writeln!(file, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        writeln!(file, "<trades>").unwrap();

        for msg_num in 0..messages_per_file {
            let trade = generate_trade(&mut rng, file_num, msg_num);
            writeln!(file, "  <message>").unwrap();
            writeln!(file, "{}", trade).unwrap();
            writeln!(file, "  </message>").unwrap();
        }

        writeln!(file, "</trades>").unwrap();

        if (file_num + 1) % 10 == 0 {
            println!("Generated {} files...", file_num + 1);
        }
    }

    println!("✅ Done! Generated {} files with {} total messages", num_files, num_files * messages_per_file);
}

fn generate_trade(rng: &mut impl Rng, file_num: usize, msg_num: usize) -> String {
    let symbols = ["AAPL", "GOOGL", "MSFT", "AMZN", "TSLA", "META", "NVDA", "JPM", "BAC", "GS"];
    let sides = ["BUY", "SELL"];
    let desks = ["Equities", "Fixed Income", "FX", "Commodities", "Derivatives"];
    let venues = ["NYSE", "NASDAQ", "LSE", "HKEX", "JPX"];
    let traders = ["Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry"];

    let symbol = symbols[rng.gen_range(0..symbols.len())];
    let side = sides[rng.gen_range(0..2)];
    let desk = desks[rng.gen_range(0..desks.len())];
    let venue = venues[rng.gen_range(0..venues.len())];
    let trader = traders[rng.gen_range(0..traders.len())];
    
    let quantity = rng.gen_range(1..10000);
    let price = rng.gen_range(10.0..5000.0);
    
    let trade_id = format!("TRD-{:06}-{:05}", file_num, msg_num);
    let trader_id = format!("T-{}", rng.gen_range(10000..99999));

    format!(
        r#"    <trade>
      <id>{}</id>
      <timestamp>2024-01-15T{}:{}:{:02}Z</timestamp>
      <symbol>{}</symbol>
      <quantity>{}</quantity>
      <price>{:.2}</price>
      <side>{}</side>
      <trader>
        <id>{}</id>
        <name>{}</name>
        <desk>{}</desk>
      </trader>
      <venue>{}</venue>
      <status>FILLED</status>
    </trade>"#,
        trade_id,
        rng.gen_range(9..17),
        rng.gen_range(0..60),
        rng.gen_range(0..60),
        symbol,
        quantity,
        price,
        side,
        trader_id,
        trader,
        desk,
        venue
    )
}
