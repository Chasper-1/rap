use std::num::NonZero;
use std::sync::mpsc::Sender;

use rodio::Source;

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
    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.input.try_seek(pos)
    }
}

impl<S> Iterator for VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next();
        if let Some(ref s) = sample {
            let clone: f32 = s.clone().into();
            let _ = self.sender.send(clone);
        }
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    use std::time::Duration;

    struct TestSource {
        samples: Vec<f32>,
        pos: usize,
    }

    impl Iterator for TestSource {
        type Item = f32;
        fn next(&mut self) -> Option<Self::Item> {
            let s = self.samples.get(self.pos).cloned();
            self.pos += 1;
            s
        }
    }

    impl Source for TestSource {
        fn channels(&self) -> NonZero<u16> {
            NonZero::new(2).unwrap()
        }
        fn sample_rate(&self) -> NonZero<u32> {
            NonZero::new(48000).unwrap()
        }
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    #[test]
    fn forwards_samples_to_sender() {
        let (tx, rx) = std::sync::mpsc::channel::<f32>();
        let src = TestSource {
            samples: vec![0.1, 0.2, 0.3],
            pos: 0,
        };
        let mut viz = VisualizableSource { input: src, sender: tx };

        assert_eq!(viz.next(), Some(0.1));
        assert_eq!(viz.next(), Some(0.2));
        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), 0.1);
        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), 0.2);
        assert_eq!(viz.next(), Some(0.3));
        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), 0.3);
        assert_eq!(viz.next(), None);

        drop(viz); // sender умирает -> канал закрывается
        assert!(rx.recv().is_err());
    }
}