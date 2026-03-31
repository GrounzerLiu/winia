use crate::shared::{SharedDerived, SharedSource};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::broadcast::{Receiver, Sender};

#[derive(Clone)]
pub struct SharedFlow<T: Clone + Send + Sync> {
    replay: usize,
    replay_cache: Arc<Mutex<VecDeque<T>>>,
    sender: Sender<T>
}

#[derive(Clone, Copy)]
pub struct SharedFlowConfig {
    /// Number of values to replay to new subscribers
    pub replay: usize,
    /// Size of the buffer for storing emitted values
    pub buffer_size: usize,
}

impl Default for SharedFlowConfig {
    fn default() -> Self {
        Self {
            replay: 0,
            buffer_size: 16,
        }
    }
}

impl SharedFlowConfig {
    pub fn replay(mut self, replay: usize) -> Self {
        self.replay = replay;
        self
    }

    pub fn buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }
}

impl<T: Clone + Send + Sync> SharedFlow<T> {
    pub fn new(config: SharedFlowConfig) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(config.buffer_size);
        Self {
            replay: config.replay,
            replay_cache: Arc::new(Mutex::new(VecDeque::with_capacity(config.replay))),
            sender
        }
    }

    pub fn send(&self, value: T) {
        if self.replay > 0 {
            let mut replay_cache = self.replay_cache.lock();
            if replay_cache.len() == self.replay {
                replay_cache.pop_front();
            }
            replay_cache.push_back(value.clone());
        }
        let _ = self.sender.send(value);
    }

    pub fn subscribe(&self) -> SharedFlowSubscription<T> {
        let receiver = self.sender.subscribe();
        let replay_values = {
            let replay_cache = self.replay_cache.lock();
            replay_cache.iter().cloned().collect::<Vec<T>>().into_iter()
        };
        SharedFlowSubscription {
            receiver,
            replay_values,
        }
    }
}

impl<T: Clone + Send + Sync + 'static> SharedFlow<T> {

    pub fn as_shared(&self, init: T) -> SharedDerived<T> {
        let shared = SharedSource::new(init);
        let shared_clone = shared.clone();
        let sub = self.subscribe();
        tokio::spawn(async move {
            let mut sub = sub;
            loop {
                match sub.recv().await {
                    Ok(value) => {
                        shared_clone.set(value);
                    }
                    Err(RecvError::Closed) => {
                        break;
                    }
                    Err(RecvError::Lagged(_)) => {
                        // Ignore lagged errors
                    }
                }
            }
        });
        shared.into()
    }
}

pub struct SharedFlowSubscription<T: Clone + Send + Sync> {
    receiver: Receiver<T>,
    replay_values: std::vec::IntoIter<T>,
}

impl<T: Clone + Send + Sync> SharedFlowSubscription<T> {
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        if let Some(value) = self.replay_values.next() {
            return Ok(value);
        }
        self.receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        if let Some(value) = self.replay_values.next() {
            return Ok(value);
        }
        self.receiver.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_shared_flow() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        flow.send(1);
        flow.send(2);
        let mut sub1 = flow.subscribe();
        assert_eq!(sub1.recv().await.unwrap(), 1);
        assert_eq!(sub1.recv().await.unwrap(), 2);
        flow.send(3);
        assert_eq!(sub1.recv().await.unwrap(), 3);
        let mut sub2 = flow.subscribe();
        assert_eq!(sub2.recv().await.unwrap(), 2);
        assert_eq!(sub2.recv().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_shared_flow_try_recv() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        flow.send(1);
        flow.send(2);
        let mut sub1 = flow.subscribe();
        assert_eq!(sub1.try_recv().unwrap(), 1);
        assert_eq!(sub1.try_recv().unwrap(), 2);
        flow.send(3);
        assert_eq!(sub1.try_recv().unwrap(), 3);
        let mut sub2 = flow.subscribe();
        assert_eq!(sub2.try_recv().unwrap(), 2);
        assert_eq!(sub2.try_recv().unwrap(), 3);
    }


    #[tokio::test]
    async fn test_shared_flow_concurrent() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        let flow_clone = flow.clone();
        let task = tokio::spawn(async move {
            for i in 1..=5 {
                flow_clone.send(i);
            }
        });
        task.await.unwrap();
        let mut sub = flow.subscribe();
        for expected in 4..=5 {
            assert_eq!(sub.recv().await.unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn test_shared_flow_concurrent2() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        let flow_clone = flow.clone();
        tokio::spawn(async move {
            for i in 1..=5 {
                flow_clone.send(i);
            }
        });
        let mut sub = flow.subscribe();
        drop(flow);
        let expected = vec![1, 2, 3, 4, 5];
        let mut received = Vec::new();
        while let Ok(value) = sub.recv().await {
            received.push(value);
        }
        assert_eq!(received, expected);
    }
    #[tokio::test]
    async fn test_shared_flow_multiple_subscribers() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        let mut sub1 = flow.subscribe();
        let mut sub2 = flow.subscribe();
        let mut expected_values1 = vec![1, 2, 3, 4, 5].into_iter();
        let mut expected_values2 = vec![1, 2, 3, 4, 5].into_iter();
        let task1 = tokio::spawn(async move {
            while let Ok(value) = sub1.recv().await {
                let expected = expected_values1.next().unwrap();
                assert_eq!(value, expected);
            }
        });
        let task2 = tokio::spawn(async move {
            while let Ok(value) = sub2.recv().await {
                let expected = expected_values2.next().unwrap();
                assert_eq!(value, expected);
            }
        });

        sleep(Duration::from_millis(100)).await;
        for i in 1..=5 {
            flow.send(i);
        }
        drop(flow);
        task1.await.unwrap();
        task2.await.unwrap();
    }

    #[tokio::test]
    async fn test_shared_flow_multiple_senders_and_subscribers() {
        let flow = SharedFlow::new(SharedFlowConfig::default().replay(2));
        let flow_clone1 = flow.clone();
        let flow_clone2 = flow.clone();

        let mut sub1 = flow.subscribe();
        let mut sub2 = flow.subscribe();

        let task1 = tokio::spawn(async move {
            for i in 1..=5 {
                flow_clone1.send(i);
            }
        });

        let task2 = tokio::spawn(async move {
            for i in 6..=10 {
                flow_clone2.send(i);
            }
        });

        let task3 = tokio::spawn(async move {
            let mut received = Vec::new();
            while let Ok(value) = sub1.recv().await {
                received.push(value);
            }
            received
        });

        let task4 = tokio::spawn(async move {
            let mut received = Vec::new();
            while let Ok(value) = sub2.recv().await {
                received.push(value);
            }
            received
        });

        task1.await.unwrap();
        task2.await.unwrap();
        drop(flow);

        let received1 = task3.await.unwrap();
        let received2 = task4.await.unwrap();

        let expected: Vec<i32> = (1..=10).collect();
        assert_eq!(received1, expected);
        assert_eq!(received2, expected);
    }
}