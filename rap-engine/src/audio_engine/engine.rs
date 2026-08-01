use super::commands::AudioCmd;
use super::source_factory;
use super::status::EngineStatus;

use anyhow::{Context, anyhow};
use std::num::NonZero;
use std::time::Duration;
use tokio::sync::{mpsc as tokio_mpsc, watch};

use rodio::Player;
use rodio::cpal::traits::HostTrait;
use rodio::stream::DeviceSinkBuilder;

/// Длительность плавного изменения громкости (фейд-вход/выход).
const FADE: Duration = Duration::from_millis(40);
/// Число шагов фейда.
const FADE_STEPS: u32 = 12;

/// Плавно (но быстро) меняет громкость плеера от текущей к `target`.
/// Вызывается только из потока движка.
async fn fade_to(player: &Player, target: f32) {
    let from = player.volume();
    if (from - target).abs() < 1e-4 {
        return;
    }
    for i in 1..=FADE_STEPS {
        let t = i as f32 / FADE_STEPS as f32;
        player.set_volume(from + (target - from) * t);
        tokio::time::sleep(FADE / FADE_STEPS).await;
    }
    player.set_volume(target);
}

/// Цикл обработки команд движка. Любая ошибка (мёртвый интерфейс,
/// сбой декодера) прерывает цикл и всплывает к месту запуска задачи.
async fn engine_loop(
    mut cmd_rx: tokio_mpsc::Receiver<AudioCmd>,
    status_tx: watch::Sender<EngineStatus>,
    mut shutdown_rx: watch::Receiver<bool>,
    player: Player,
) -> anyhow::Result<()> {
    // Громкость, которую выставил пользователь: после фейдов
    // плеер возвращается именно к ней.
    let mut target_gain: f32 = 1.0;

    // Таймер статуса живёт вне select: в отличие от sleep(), который
    // пересоздаётся на каждой команде, interval срабатывает строго
    // каждые 50 мс даже при плотном потоке команд (зажатая клавиша).
    let mut status_timer = tokio::time::interval(Duration::from_millis(50));

    loop {
        tokio::select! {
            // Команды от UI
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    AudioCmd::Play { path, channels } => {
                        if let Some(src) = source_factory::open_source(&path, channels).await {
                            // Сначала плавно гасим старый трек, чтобы не было щелчка
                            fade_to(&player, 0.0).await;
                            player.stop();
                            player.append(src);
                            player.play();
                            // И плавно поднимаем громкость нового трека
                            fade_to(&player, target_gain).await;
                        }
                    }
                    AudioCmd::PlayPaused { path, channels, seek_secs } => {
                        if let Some(src) = source_factory::open_source(&path, channels).await {
                            fade_to(&player, 0.0).await;
                            // Пауза ставится ДО добавления источника: трек
                            // добавляется уже в паузе и не успевает зазвучать.
                            player.pause();
                            player.stop();
                            player.append(src);
                            if seek_secs > 0 {
                                player
                                    .try_seek(Duration::from_secs(seek_secs))
                                    .context("не удалось перемотать на старте")?;
                            }
                        }
                    }
                    AudioCmd::Stop => {
                        fade_to(&player, 0.0).await;
                        player.stop();
                    }
                    AudioCmd::Pause => {
                        fade_to(&player, 0.0).await;
                        player.pause();
                    }
                    AudioCmd::Resume => {
                        player.play();
                        fade_to(&player, target_gain).await;
                    }
                    AudioCmd::Volume(v) => {
                        target_gain = v;
                        player.set_volume(v);
                    }
                    AudioCmd::Seek(d) => {
                        player.try_seek(d).context("не удалось перемотать")?;
                        // Сразу публикуем новую позицию, не дожидаясь тика статуса
                        status_tx
                            .send(EngineStatus {
                                position: player.get_pos(),
                                is_paused: player.is_paused(),
                                volume: player.volume(),
                                is_empty: player.empty(),
                            })
                            .map_err(|_| anyhow!("получатель статуса закрыт"))?;
                    }
                    AudioCmd::SeekRelative(offset) => {
                        // Считаем от реальной позиции плеера в его же потоке:
                        // при зажатой клавише повторные сдвиги не «застревают»
                        // на устаревшей позиции из статуса.
                        let current = player.get_pos();
                        let target = if offset >= 0 {
                            current
                                .saturating_add(Duration::from_secs(offset as u64))
                        } else {
                            current
                                .saturating_sub(Duration::from_secs(offset.unsigned_abs()))
                        };
                        player.try_seek(target).context("не удалось перемотать")?;
                        // Сразу публикуем новую позицию, не дожидаясь тика статуса
                        status_tx
                            .send(EngineStatus {
                                position: player.get_pos(),
                                is_paused: player.is_paused(),
                                volume: player.volume(),
                                is_empty: player.empty(),
                            })
                            .map_err(|_| anyhow!("получатель статуса закрыт"))?;
                    }
                }
            }
            // Сигнал завершения
            Ok(()) = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    player.stop();
                    break;
                }
            }
            // Периодическое обновление статуса (50 мс — полоска
            // прогресса движется плавно, в т.ч. при зажатой перемотке)
            _ = status_timer.tick() => {
                status_tx
                    .send(EngineStatus {
                        position: player.get_pos(),
                        is_paused: player.is_paused(),
                        volume: player.volume(),
                        is_empty: player.empty(),
                    })
                    .map_err(|_| anyhow!("получатель статуса закрыт"))?;
            }
        }
    }
    // Даём время на освобождение аудиоустройства
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

pub struct AudioEngine {
    cmd_tx: tokio_mpsc::Sender<AudioCmd>,
    status_rx: watch::Receiver<EngineStatus>,
    shutdown_tx: watch::Sender<bool>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = tokio_mpsc::channel::<AudioCmd>(64);
        let (status_tx, status_rx) = watch::channel(EngineStatus::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let task_handle = tokio::spawn(async move {
            let host = rodio::cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output device found.");

            let mut stream = DeviceSinkBuilder::from_device(device)
                .expect("Failed to create SinkBuilder")
                .with_sample_rate(NonZero::new(48000).unwrap())
                .open_sink_or_fallback()
                .expect("System Error: Failed to open audio sink.");

            stream.log_on_drop(false);

            let player = Player::connect_new(stream.mixer());

            if let Err(e) = engine_loop(cmd_rx, status_tx, shutdown_rx, player).await {
                eprintln!("движок остановлен с ошибкой: {e}");
            }
        });

        Self {
            cmd_tx,
            status_rx,
            shutdown_tx,
            task_handle: Some(task_handle),
        }
    }

    // --- ПУБЛИЧНЫЕ МЕТОДЫ ---

    pub async fn play(&self, path: &str) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Play {
                path: path.to_string(),
                channels: 2,
            })
            .await
            .map_err(|_| anyhow!("движок не принял команду play"))
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Stop)
            .await
            .map_err(|_| anyhow!("движок не принял команду stop"))
    }

    /// Запуск трека сразу на паузе (для продолжения с сохранённой позиции).
    pub async fn play_paused(&self, path: &str, seek_secs: u64) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::PlayPaused {
                path: path.to_string(),
                channels: 2,
                seek_secs,
            })
            .await
            .map_err(|_| anyhow!("движок не принял команду play_paused"))
    }

    pub async fn pause(&self) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Pause)
            .await
            .map_err(|_| anyhow!("движок не принял команду pause"))
    }

    pub async fn resume(&self) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Resume)
            .await
            .map_err(|_| anyhow!("движок не принял команду resume"))
    }

    pub fn is_paused(&self) -> bool {
        self.status_rx.borrow().is_paused
    }
    pub fn is_empty(&self) -> bool {
        self.status_rx.borrow().is_empty
    }
    pub fn get_current_pos(&self) -> u64 {
        self.status_rx.borrow().position.as_secs()
    }

    pub fn get_volume(&self) -> f32 {
        gain_to_volume(self.status_rx.borrow().volume)
    }

    pub async fn set_volume(&self, vol: f32) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Volume(volume_to_gain(vol)))
            .await
            .map_err(|_| anyhow!("движок не принял команду set_volume"))
    }

    pub async fn seek_to(&self, seconds: u64) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::Seek(Duration::from_secs(seconds)))
            .await
            .map_err(|_| anyhow!("движок не принял команду seek_to"))
    }

    pub async fn seek_relative(&self, offset_secs: i64) -> anyhow::Result<()> {
        self.cmd_tx
            .send(AudioCmd::SeekRelative(offset_secs))
            .await
            .map_err(|_| anyhow!("движок не принял команду seek_relative"))
    }

    /// Сигнализирует движку о завершении работы и ждёт его остановки.
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.shutdown_tx
            .send(true)
            .map_err(|_| anyhow!("движок уже остановлен"))?;
        if let Some(handle) = self.task_handle.take() {
            tokio::time::timeout(Duration::from_secs(2), handle)
                .await
                .map_err(|_| anyhow!("движок не остановился за 2 секунды"))??;
        }
        Ok(())
    }
}

/// Конвертирует gain (0..1 у rodio) в «человеческий» уровень громкости 0..1.
pub fn gain_to_volume(gain: f32) -> f32 {
    if gain <= 0.0 {
        return 0.0;
    }
    (f32::ln(gain * (f32::exp(5.0) - 1.0) + 1.0) / 5.0).clamp(0.0, 1.0)
}

/// Конвертирует «человеческий» уровень громкости 0..1 в gain для rodio.
pub fn volume_to_gain(vol: f32) -> f32 {
    let v = vol.clamp(0.0, 1.0);
    (f32::exp(v * 5.0) - 1.0) / (f32::exp(5.0) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_to_volume_zero() {
        assert_eq!(gain_to_volume(0.0), 0.0);
        assert_eq!(gain_to_volume(-1.0), 0.0);
    }

    #[test]
    fn gain_to_volume_max() {
        assert!((gain_to_volume(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn volume_round_trip() {
        for vol in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let gain = volume_to_gain(vol);
            let back = gain_to_volume(gain);
            assert!((back - vol).abs() < 1e-6, "vol={} back={}", vol, back);
        }
    }

    #[test]
    fn volume_monotonic() {
        let mut prev = gain_to_volume(0.0);
        for g in 0..=100 {
            let v = gain_to_volume(g as f32 / 100.0);
            assert!(v >= prev - 1e-9);
            prev = v;
        }
    }
}
