use rodio::Source;
use tokio::sync::mpsc::Sender;
use std::num::NonZero; // Нужно для типов NonZero

pub struct VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    pub input: S,
    pub sender: Sender<f32>,
}

impl<S> Source for VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    // В новых версиях возвращаем NonZero напрямую из input
    fn channels(&self) -> NonZero<u16> { self.input.channels() }
    fn sample_rate(&self) -> NonZero<u32> { self.input.sample_rate() }
    
    fn current_span_len(&self) -> Option<usize> { self.input.current_span_len() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.input.total_duration() }
}

impl<S> Iterator for VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        // Копируем сэмпл в канал для CAVA
        let _ = self.sender.try_send(sample.into());
        Some(sample)
    }
}