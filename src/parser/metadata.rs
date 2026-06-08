use lofty::prelude::*;
use lofty::probe::Probe;

pub async fn get_audio_info(path: &str) -> (String, String, u32, u16) {
    let path_owned = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut info = ("Unknown".to_string(), "Unknown".to_string(), 48000, 2);
        if let Ok(probe) = Probe::open(&path_owned) {
            if let Ok(tagged_file) = probe.read() {
                let props = tagged_file.properties();
                let sample_rate = props.sample_rate().unwrap_or(48000);
                let channels = props.channels().map(|c| c as u16).unwrap_or(2);

                if let Some(t) = tagged_file
                    .primary_tag()
                    .or_else(|| tagged_file.first_tag())
                {
                    let rt = tokio::runtime::Handle::current();
                    let (artist, title) =
                        rt.block_on(crate::parser::artist::process_and_log_metadata(
                            t.artist().map(|s| s.to_string()),
                            t.title().map(|s| s.to_string()),
                            t.album().map(|s| s.to_string()),
                            t.get_string(ItemKey::Year).map(|s| s.to_string()),
                            t.genre().map(|s| s.to_string()),
                            t.get_string(ItemKey::Comment).map(|s| s.to_string()),
                        ));
                    info = (artist, title, sample_rate, channels);
                } else {
                    info = ("Unknown".into(), "Unknown".into(), sample_rate, channels);
                }
            }
        }
        info
    })
    .await
    .unwrap_or(("Unknown".to_string(), "Unknown".to_string(), 48000, 2))
}