use crate::animation::Interpolator;
use crate::animation::interpolator::Linear;
use crate::app::EventLoopProxy;
use crate::core::next_id;
use parking_lot::lock_api::MutexGuard;
use parking_lot::{Mutex, RawMutex};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

pub trait Readable: Send + Sized + 'static {}
pub trait Writable: Readable {}
pub struct Derived;
impl Readable for Derived {}
pub struct Source;
impl Readable for Source {}
impl Writable for Source {}

type Generator<T> = Arc<Mutex<Option<Box<dyn FnMut() -> T + Send>>>>;
type GeneratorWeak<T> = Weak<Mutex<Option<Box<dyn FnMut() -> T + Send>>>>;
type Interceptor<T> = Arc<Mutex<HashMap<u32, Box<dyn FnMut(&mut T, T) -> Option<T> + Send>>>>;
type InterceptorWeak<T> = Weak<Mutex<HashMap<u32, Box<dyn FnMut(&mut T, T) -> Option<T> + Send>>>>;
type Observers = Arc<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>>;
type ObserversWeak = Weak<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>>;
type Dependencies = Arc<Mutex<Vec<Weak<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>>>>>;
type DependenciesWeak = Weak<Mutex<Vec<Weak<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>>>>>;
type Animation<T> = Arc<Mutex<Option<SharedAnimation<T>>>>;
type AnimationWeak<T> = Weak<Mutex<Option<SharedAnimation<T>>>>;
pub struct Shared<T, Access: Readable> {
    _access_marker: PhantomData<Access>,
    id: u32,
    value: Arc<Mutex<T>>,
    generator: Generator<T>,
    interceptor: Interceptor<T>,
    observers: Observers,
    dependencies: Dependencies,
    animation: Animation<T>,
}

pub type SharedSource<T> = Shared<T, Source>;
pub type SharedDerived<T> = Shared<T, Derived>;

impl<T, A: Readable> Clone for Shared<T, A> {
    fn clone(&self) -> Self {
        Shared {
            _access_marker: Default::default(),
            id: self.id,
            value: self.value.clone(),
            generator: self.generator.clone(),
            interceptor: self.interceptor.clone(),
            observers: self.observers.clone(),
            dependencies: self.dependencies.clone(),
            animation: self.animation.clone(),
        }
    }
}

impl<T: Default> Default for Shared<T, Source>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Shared::new(T::default())
    }
}

impl<T> Default for Shared<T, Derived>
where
    T: Default + 'static,
{
    fn default() -> Self {
        Shared::new_derived(T::default())
    }
}

impl<T, A> Debug for Shared<T, A>
where
    T: Debug + 'static,
    A: Readable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("id", &self.id)
            .field("value", &self.value.lock().deref())
            .finish()
    }
}

impl<T, A> PartialEq for Shared<T, A>
where
    T: PartialEq + 'static,
    A: Readable,
{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id || self.value.lock().deref() == other.value.lock().deref()
    }
}

impl<T, A> Eq for Shared<T, A>
where
    T: Eq + 'static,
    A: Readable,
{
}

impl<T, A> Display for Shared<T, A>
where
    T: Display + 'static,
    A: Readable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.lock().deref())
    }
}

impl<T: 'static> Shared<T, Source> {
    pub fn new(value: T) -> Self {
        Shared::<T, Source> {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(value)),
            generator: Arc::new(Mutex::new(None)),
            interceptor: Arc::new(Mutex::new(HashMap::new())),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
            animation: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T> Shared<T, Source>
where
    T: Send + 'static,
{
    pub fn from_async(
        value: impl Future<Output = Option<T>> + Send + 'static,
        default_value: T,
    ) -> Self {
        let shared = Shared::<T, Source>::new(default_value);
        let clone = shared.clone();
        tokio::spawn(async move {
            let value = value.await;
            if let Some(v) = value {
                clone.set(v);
            }
        });
        shared
    }
}

impl<T: 'static> Shared<T, Derived> {
    pub fn new_derived(value: T) -> Self {
        Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(value)),
            generator: Arc::new(Mutex::new(None)),
            interceptor: Arc::new(Mutex::new(HashMap::new())),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
            animation: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T, A> Shared<T, A>
where
    T: 'static,
    A: Writable + Readable,
{
    pub fn set(&self, new_value: impl Into<T>) {
        let mut value = self.value.lock();
        let new_value = new_value.into();
        if let Some(intercepted_value) = self.intercept(&mut value, new_value) {
            *value = intercepted_value;
            drop(value);
            self.notify();
        }
    }

    pub fn write<R>(&self, func: impl FnOnce(&mut T) -> R) -> R {
        let mut value = self.value.lock();
        let r = func(&mut value);
        drop(value);
        self.notify();
        r
    }

    pub fn notify(&self) {
        let mut observers = self.observers.lock();
        for observer in observers.values_mut() {
            observer();
        }
    }
}

impl<T, A> Shared<T, A>
where
    T: 'static,
    A: Readable,
{
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Locks the shared value for mutable access.
    /// This method will not notify observers when the inner value is changed.
    /// If you want to notify observers, use the [`set`](Shared::set) or [`write`](Shared::write) methods.
    pub fn lock(&self) -> MutexGuard<'_, RawMutex, T> {
        self.value.lock()
    }

    pub fn read(&self) -> SharedReadGuard<'_, T> {
        SharedReadGuard {
            guard: self.value.lock(),
        }
    }

    pub fn get_animation(&self) -> Option<SharedAnimation<T>> {
        self.animation.lock().clone()
    }

    fn intercept(&self, old_value: &mut T, new_value: T) -> Option<T> {
        let mut interceptor = self.interceptor.lock();
        let mut current_value = Some(new_value);
        for interceptor_fn in interceptor.values_mut() {
            if let Some(v) = current_value {
                current_value = interceptor_fn(old_value, v);
            } else {
                break;
            }
        }
        current_value
    }

    pub fn add_interceptor(
        &self,
        interceptor_id: u32,
        interceptor_fn: impl FnMut(&mut T, T) -> Option<T> + Send + 'static,
    ) {
        self.interceptor
            .lock()
            .insert(interceptor_id, Box::new(interceptor_fn));
    }
}

#[macro_export]
macro_rules! depend {
    ($($dep:expr),* $(,)?) => {
        {
            use $crate::shared::Observable;
            vec![
                $(
                    {
                        let o: Box<dyn Observable> = Box::new($dep.clone());
                        o
                    }
                ),*
            ]
        }
    }
}

impl<T> Shared<T, Derived>
where
    T: Send + 'static,
{
    pub fn from_fn<F>(dependencies: Vec<Box<dyn Observable>>, generator: F) -> Self
    where
        F: Fn() -> T + Send + 'static,
    {
        let shared = Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(generator())),
            generator: Arc::new(Mutex::new(Some(Box::new(generator)))),
            interceptor: Arc::new(Mutex::new(HashMap::new())),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
            animation: Arc::new(Mutex::new(None)),
        };
        for dependency in dependencies {
            shared.depends_on(dependency.deref());
        }
        shared
    }
}

pub struct SharedReadGuard<'a, T> {
    guard: MutexGuard<'a, RawMutex, T>,
}
impl<'a, T> Deref for SharedReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl<T, A> Shared<T, A>
where
    T: Send + 'static,
    A: Readable,
{
    pub fn subscribe(&self, observer_id: u32, callback: impl FnMut() + Send + 'static) {
        self.observers
            .lock()
            .insert(observer_id, Box::new(callback));
    }

    pub fn depends_on(&self, other: &(impl Observable + ?Sized)) {
        other.observers().lock().insert(self.id, {
            let observers_weak = Arc::downgrade(&self.observers);
            let value_generator_weak = Arc::downgrade(&self.generator);
            let value_weak = Arc::downgrade(&self.value);
            let interceptor_weak = Arc::downgrade(&self.interceptor);
            Box::new(move || {
                if let (Some(observers), Some(value_generator), Some(value), Some(interceptor)) = (
                    observers_weak.upgrade(),
                    value_generator_weak.upgrade(),
                    value_weak.upgrade(),
                    interceptor_weak.upgrade(),
                ) && let Some(generator) = &mut *value_generator.lock()
                {
                    let new_value = generator();
                    let mut current_value = Some(new_value);
                    for interceptor_fn in interceptor.lock().values_mut() {
                        if let Some(v) = current_value {
                            current_value = interceptor_fn(&mut value.lock(), v);
                        } else {
                            break;
                        }
                    }
                    if let Some(v) = current_value {
                        *value.lock() = v;
                        let mut observers = observers.lock();
                        for observer in observers.values_mut() {
                            observer();
                        }
                    }
                }
            })
        });
        self.dependencies
            .lock()
            .push(Arc::downgrade(other.observers()));
    }
}

impl<T, A> Shared<T, A>
where
    T: Send + Clone + 'static,
    A: Readable,
{
    pub fn get(&self) -> T {
        self.value.lock().clone()
    }
}
impl<T, A: Readable> Drop for Shared<T, A> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.value) == 1 {
            let mut observed = self.dependencies.lock();
            observed.iter_mut().for_each(|weak_observers| {
                if let Some(observers) = weak_observers.upgrade() {
                    observers.lock().remove(&self.id);
                }
            });
        }
    }
}

pub trait Observable {
    fn observers(&self) -> &Arc<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>>;
}

impl<T, A: Readable> Observable for Shared<T, A>
where
    T: Send + 'static,
{
    fn observers(&self) -> &Arc<Mutex<HashMap<u32, Box<dyn FnMut() + Send>>>> {
        &self.observers
    }
}

impl<T: Send + 'static> Into<Box<dyn Observable>> for &Shared<T, Source> {
    fn into(self) -> Box<dyn Observable> {
        Box::new(self.clone())
    }
}

impl<T: Send + 'static> Into<Box<dyn Observable>> for &Shared<T, Derived> {
    fn into(self) -> Box<dyn Observable> {
        Box::new(self.clone())
    }
}

impl<T: 'static> From<T> for Shared<T, Source> {
    fn from(value: T) -> Self {
        Shared::new(value)
    }
}

impl<T: 'static> From<T> for Shared<T, Derived> {
    fn from(value: T) -> Self {
        Shared::new_derived(value)
    }
}

impl<T: 'static, A: Readable> From<&Shared<T, A>> for Shared<T, Derived> {
    fn from(value: &Shared<T, A>) -> Self {
        Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: value.id,
            value: value.value.clone(),
            generator: value.generator.clone(),
            interceptor: value.interceptor.clone(),
            observers: value.observers.clone(),
            dependencies: value.dependencies.clone(),
            animation: value.animation.clone(),
        }
    }
}

impl<T: 'static> From<Shared<T, Source>> for Shared<T, Derived> {
    fn from(value: Shared<T, Source>) -> Self {
        Shared::from(&value)
    }
}

impl<T: 'static> From<T> for Shared<Option<T>, Source> {
    fn from(value: T) -> Self {
        Shared::new(Some(value))
    }
}

impl<T: 'static> From<T> for Shared<Option<T>, Derived> {
    fn from(value: T) -> Self {
        Shared::new_derived(Some(value))
    }
}

impl<T, A: Readable> AsRef<Shared<T, A>> for Shared<T, A> {
    fn as_ref(&self) -> &Shared<T, A> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Item;
    use crate::ui::item::ItemKind;
    use std::thread;

    #[derive(Clone)]
    struct UnSend {
        ptr: *const u8,
    }

    #[test]
    fn test_shared() {
        let sendable = Shared::new(5);
        let a = 8;
        let unsendable = Shared::new(UnSend {
            ptr: std::ptr::addr_of!(a),
        });

        thread::spawn(move || {
            let a = sendable.get();
            println!("Value from sendable: {}", a);
            // The following line would cause a compile-time error if uncommented
            // let _ = unsendable.read();
        })
        .join()
        .unwrap();
    }
}

#[derive(Clone)]
pub struct SharedWeak<T, Access: Readable> {
    _access_marker: PhantomData<Access>,
    id: u32,
    value: Weak<Mutex<T>>,
    generator: GeneratorWeak<T>,
    interceptor: InterceptorWeak<T>,
    observers: ObserversWeak,
    dependencies: DependenciesWeak,
    animation: AnimationWeak<T>,
}

impl<T, A> Shared<T, A>
where
    T: 'static,
    A: Readable,
{
    pub fn weak(&self) -> SharedWeak<T, A> {
        SharedWeak {
            _access_marker: Default::default(),
            id: self.id,
            value: Arc::downgrade(&self.value),
            generator: Arc::downgrade(&self.generator),
            interceptor: Arc::downgrade(&self.interceptor),
            observers: Arc::downgrade(&self.observers),
            dependencies: Arc::downgrade(&self.dependencies),
            animation: Arc::downgrade(&self.animation),
        }
    }
}

impl<T, A> SharedWeak<T, A>
where
    T: 'static,
    A: Readable,
{
    pub fn upgrade(&self) -> Option<Shared<T, A>> {
        Some(Shared {
            _access_marker: Default::default(),
            id: self.id,
            value: self.value.upgrade()?,
            generator: self.generator.upgrade()?,
            interceptor: self.interceptor.upgrade()?,
            observers: self.observers.upgrade()?,
            dependencies: self.dependencies.upgrade()?,
            animation: self.animation.upgrade()?,
        })
    }
}

struct InnerSharedAnimation<T> {
    id: u32,
    is_stopped: bool,
    is_finished: bool,
    enable_repeat: bool,
    shared: SharedWeak<T, Source>,
    from: T,
    to: T,
    value_generator: Box<dyn Fn(&T, &T, f32) -> T + Send>,
    duration: Duration,
    start_time: Instant,
    interpolator: Box<dyn Interpolator + Send>,
    on_start: Option<Box<dyn FnMut() + Send>>,
    on_finish: Option<Box<dyn FnMut() + Send>>,
}

impl<T: Send + 'static> InnerSharedAnimation<T> {
    pub fn new(
        f32: SharedSource<T>,
        from: T,
        to: T,
        value_generator: impl Fn(&T, &T, f32) -> T + Send + 'static,
    ) -> Self {
        Self {
            id: next_id(),
            is_stopped: false,
            is_finished: false,
            enable_repeat: false,
            shared: f32.weak(),
            from,
            to,
            value_generator: Box::new(value_generator),
            duration: Duration::from_secs(500),
            start_time: Instant::now(),
            interpolator: Box::new(Linear::new()),
            on_start: None,
            on_finish: None,
        }
    }

    pub fn enable_repeat(&mut self) {
        self.enable_repeat = true;
    }

    pub fn duration(&mut self, duration: Duration) {
        self.duration = duration;
    }

    pub fn interpolator(&mut self, interpolator: impl Interpolator + Send + 'static) {
        self.interpolator = Box::new(interpolator);
    }

    pub fn on_start(&mut self, on_start: impl FnMut() + Send + 'static) {
        self.on_start = Some(Box::new(on_start));
    }

    /// Set the function to be called when the animation is finished or stopped.
    pub fn on_finish(&mut self, on_finish: impl FnMut() + Send + 'static) {
        self.on_finish = Some(Box::new(on_finish));
    }

    // pub fn start(mut self, app_context: &AppContext){
    //     self.start_time = Instant::now();
    //     app_context.shared_animations.value().push(Box::new(self));
    //     app_context.request_redraw();
    //     if let Some(on_start) = self.on_start.take(){
    //         on_start();
    //     }
    // }

    pub fn stop(&mut self) {
        self.is_stopped = true;
        // if let Some(on_finish) = self.on_finish.as_mut(){
        //     on_finish();
        // }
    }

    pub fn get_finished(&mut self) -> bool {
        if self.enable_repeat {
            if self.is_stopped {
                self.is_finished = true;
                true
            } else if self.start_time.elapsed() >= self.duration {
                self.start_time = Instant::now();
                self.is_finished = false;
                false
            } else {
                self.is_finished = false;
                false
            }
        } else {
            let is_finished = self.is_stopped || self.start_time.elapsed() >= self.duration;
            if !self.is_finished && is_finished {
                if let Some(on_finish) = self.on_finish.as_mut() {
                    on_finish();
                }
            }
            self.is_finished = is_finished;
            is_finished
        }
    }

    pub fn update(&mut self) {
        if self.get_finished() {
            let new_value = (self.value_generator)(&self.from, &self.to, 1.0);
            if let Some(shared) = self.shared.upgrade() {
                shared.set(new_value);
            }
            return;
        }
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        let new_value = (self.value_generator)(&self.from, &self.to, interpolated);
        if let Some(shared) = self.shared.upgrade() {
            shared.set(new_value);
        }
    }
}

pub struct SharedAnimation<T> {
    inner: Arc<Mutex<InnerSharedAnimation<T>>>,
}

impl<T: Send + 'static> SharedAnimation<T> {
    pub fn new(
        f32: SharedSource<T>,
        from: T,
        to: T,
        value_generator: impl Fn(&T, &T, f32) -> T + Send + 'static,
    ) -> Self {
        let inner = InnerSharedAnimation::new(f32, from, to, value_generator);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn enable_repeat(self) -> Self {
        self.inner.lock().enable_repeat();
        self
    }

    pub fn duration(self, duration: Duration) -> Self {
        self.inner.lock().duration = duration;
        self
    }

    pub fn interpolator(self, interpolator: impl Interpolator + Send + 'static) -> Self {
        self.inner.lock().interpolator(interpolator);
        self
    }

    pub fn on_start(self, on_start: impl FnMut() + Send + 'static) -> Self {
        self.inner.lock().on_start(on_start);
        self
    }

    pub fn on_finish(self, on_finish: impl FnMut() + Send + 'static) -> Self {
        self.inner.lock().on_finish(on_finish);
        self
    }

    pub fn start(self, event_loop_proxy: &EventLoopProxy) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.start_time = Instant::now();
            event_loop_proxy.start_shared_animation(Box::new(self.clone()));
            let cloned = self.clone();
            if let Some(shared) = inner.shared.upgrade() {
                shared.animation.lock().replace(cloned);
            }
            if let Some(mut on_start) = inner.on_start.take() {
                on_start();
            }
        }
        self
    }

    pub fn start_delayed(self, event_loop_proxy: &EventLoopProxy, delay: Duration) -> Self {
        let event_loop_proxy = event_loop_proxy.clone();
        let self_clone = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            self_clone.start(&event_loop_proxy);
        });
        self
    }

    pub fn cancel(&mut self) {
        self.inner.lock().on_finish.take();
        self.stop()
    }

    pub fn stop(&mut self) {
        self.inner.lock().stop();
    }

    pub fn is_finished(&self) -> bool {
        self.inner.lock().get_finished()
    }

    pub fn id(&self) -> u32 {
        self.inner.lock().id
    }
}

impl<T> Clone for SharedAnimation<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) trait SharedAnimationTrait {
    fn is_finished(&self) -> bool;
    fn update(&self);
}

impl<T: Send + 'static> SharedAnimationTrait for SharedAnimation<T> {
    fn is_finished(&self) -> bool {
        self.inner.lock().get_finished()
    }

    fn update(&self) {
        self.inner.lock().update();
    }
}
