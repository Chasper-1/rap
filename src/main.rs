mod audio_engine;
mod logger;

use audio_engine::AudioEngine;
use std::env;
use std::io::{self, Write};
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: rmpt <path>");
        return;
    }
    let path = args.join(" ");
    
    let engine = AudioEngine::new();
    let (artist, title) = engine.play(&path).await;
    
    println!("\n▶ {} - {}", artist, title);
    let start_time = Instant::now();
    
    while !engine.is_empty() {
        let elapsed = start_time.elapsed().as_secs();
        let minutes = elapsed / 60;
        let seconds = elapsed % 60;
        
        // Печатаем прогресс, не мешая логам (если они прилетят выше)
        print!("\r[⏳ {:02}:{:02}] ", minutes, seconds);
        io::stdout().flush().unwrap();
        
        sleep(Duration::from_millis(200)).await;
    }
}