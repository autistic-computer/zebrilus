use parking_lot::Mutex;

type ReplyCallback<T> = Box<dyn FnOnce(Option<T>) + Send>;

pub struct Reply<T> {
    callback: Mutex<Option<ReplyCallback<T>>>,
}

impl<T> std::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply").finish_non_exhaustive()
    }
}

impl<T> Reply<T> {
    pub fn new(callback: impl FnOnce(Option<T>) + Send + 'static) -> Self {
        Self {
            callback: Mutex::new(Some(Box::new(callback))),
        }
    }

    pub fn try_send(&self, value: T) {
        if let Some(callback) = self.callback.lock().take() {
            callback(Some(value));
        }
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.get_mut().take() {
            callback(None);
        }
    }
}

impl<T: Send + 'static> From<crossbeam_channel::Sender<T>> for Reply<T> {
    fn from(sender: crossbeam_channel::Sender<T>) -> Self {
        Self::new(move |value| {
            if let Some(value) = value {
                let _ = sender.try_send(value);
            }
        })
    }
}

impl<T: Send + 'static> From<std::sync::mpsc::Sender<T>> for Reply<T> {
    fn from(sender: std::sync::mpsc::Sender<T>) -> Self {
        Self::new(move |value| {
            if let Some(value) = value {
                let _ = sender.send(value);
            }
        })
    }
}
