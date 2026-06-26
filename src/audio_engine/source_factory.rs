

async fn open_source(path: &str, channels: u16) -> Option<Box<dyn Source + Send>> {
    let p = path.to_string();
    tokio::task::spawn_blocking(move || {
        let file = File::open(&p).ok()?;
        if p.to_lowercase().ends_with(".opus") {
            return OpusSource::new(BufReader::new(file), channels)
                .map(|s| Box::new(s) as Box<dyn Source + Send>);
        }
        SymphoniaSource::new(file).map(|s| Box::new(s) as Box<dyn Source + Send>)
    })
    .await
    .ok()?
}