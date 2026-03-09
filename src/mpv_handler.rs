use mpv::{MpvHandler, MpvHandlerBuilder, Event};
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};

struct SendMpv(MpvHandler);
unsafe impl Send for SendMpv {}
unsafe impl Sync for SendMpv {}

#[derive(Debug)]
pub enum PlayerEvent {
    TimePos(f64),
    MetadataUpdate { artist: String, title: String },
    TrackStarted,
    EndFile(String), 
}

pub struct MpvController {
    handle: Arc<Mutex<SendMpv>>,
}

impl MpvController {
    pub fn new() -> (Self, mpsc::Receiver<PlayerEvent>) {
        let mut builder = MpvHandlerBuilder::new().expect("libmpv init failed");
        
        builder.set_option("vo", "null").ok();
        builder.set_option("ao", "pipewire").ok(); 
        builder.set_option("audio-display", "no").ok();
        builder.set_option("input-default-bindings", "yes").ok();
        builder.set_option("idle", "yes").ok();

        let mut handle_inner = builder.build().expect("mpv build failed");
        handle_inner.observe_property::<f64>("time-pos", 0).ok();
        
        let handle = Arc::new(Mutex::new(SendMpv(handle_inner)));
        let (tx, rx) = mpsc::channel::<PlayerEvent>(64);

        let h_clone: Arc<Mutex<SendMpv>> = Arc::clone(&handle);
        std::thread::spawn(move || {
            loop {
                let event = {
                    let mut guard = h_clone.lock().unwrap();
                    guard.0.wait_event(0.1)
                };

                if let Some(event) = event {
                    match event {
                        Event::StartFile => {
                            let _ = tx.blocking_send(PlayerEvent::TrackStarted);
                        }
                        Event::PropertyChange { name, .. } if name == "time-pos" => {
                            let guard = h_clone.lock().unwrap();
                            if let Ok(pos) = guard.0.get_property::<f64>("time-pos") {
                                let _ = tx.blocking_send(PlayerEvent::TimePos(pos));
                            }
                        }
                        Event::MetadataUpdate => {
                            let guard = h_clone.lock().unwrap();
                            let artist: String = guard.0.get_property::<&str>("metadata/by-key/artist")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| "Unknown Artist".to_string());
                            let title: String = guard.0.get_property::<&str>("metadata/by-key/title")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| "Unknown Title".to_string());
                            
                            let _ = tx.blocking_send(PlayerEvent::MetadataUpdate { artist, title });
                        }
                        Event::EndFile(reason) => {
                            // Просто выводим дебаг-представление того, что дала либа
                            let reason_str = format!("{:?}", reason);
                            let _ = tx.blocking_send(PlayerEvent::EndFile(reason_str));
                        }
                        _ => {}
                    }
                }
            }
        });

        (Self { handle }, rx)
    }

    pub fn load(&self, path: &str) {
        let mut guard = self.handle.lock().unwrap();
        // Используем 'replace', чтобы старый поток (если он завис) выбило нафиг
        guard.0.command(&["loadfile", path, "replace"]).expect("Failed to load path");
        guard.0.set_property("pause", false).ok();
    }
}