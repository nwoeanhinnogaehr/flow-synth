use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{atomic::{AtomicBool, Ordering},
                Arc};

use crossbeam::sync::SegQueue;

use futures::prelude::*;
use futures::task;

pub struct FutureWrap<F: Future, T> {
    inner: F,
    value: Option<T>,
}
impl<F: Future, T> Future for FutureWrap<F, T> {
    type Item = (T, F::Item);
    type Error = (T, F::Error);
    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll(cx) {
            Ok(Async::Ready(item)) => Ok(Async::Ready((self.value.take().unwrap(), item))),
            Ok(Async::Pending) => Ok(Async::Pending),
            Err(err) => Err((self.value.take().unwrap(), err)),
        }
    }
}
pub trait FutureWrapExt: Future {
    fn wrap<T>(self, value: T) -> FutureWrap<Self, T>
    where
        Self: Sized,
    {
        FutureWrap {
            inner: self,
            value: Some(value),
        }
    }
}
impl<T: Future + ?Sized> FutureWrapExt for T {}

#[derive(Clone)]
pub struct Breaker {
    broken: Arc<AtomicBool>,
}
impl Breaker {
    pub fn new() -> Breaker {
        Breaker {
            broken: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn brake(&self) {
        self.broken.store(true, Ordering::Relaxed);
    }
    pub fn test(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }
}

/// A lock/mutex where attempting to lock produces a Future
/// But you can also spin with `spin_lock` or try with `try_lock`
///
/// TODO think about/implement poisoning
pub struct Lock<T> {
    flag: AtomicBool,
    queue: SegQueue<task::Waker>,
    data: UnsafeCell<T>,
}
impl<T> Lock<T> {
    pub fn new(data: T) -> Lock<T> {
        Lock {
            flag: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            queue: SegQueue::new(),
        }
    }
    pub fn lock(&self) -> LockFuture<T> {
        LockFuture {
            lock: self,
        }
    }
    pub fn try_lock(&self) -> Option<LockGuard<T>> {
        if self.flag.compare_and_swap(false, true, Ordering::Acquire) == false {
            Some(LockGuard {
                lock: self,
            })
        } else {
            None
        }
    }
    pub fn spin_lock(&self) -> LockGuard<T> {
        while self.flag.compare_and_swap(false, true, Ordering::Acquire) {}
        LockGuard {
            lock: self,
        }
    }
}
pub struct LockFuture<'a, T: 'a> {
    lock: &'a Lock<T>,
}
impl<'a, T: 'a> Future for LockFuture<'a, T> {
    type Item = LockGuard<'a, T>;
    type Error = Never;

    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        for try in 0..2 {
            if self.lock.flag.compare_and_swap(false, true, Ordering::Acquire) {
                // if we failed to lock, register this future to be notified upon next release and
                // try again in case it gets unlocked in between trying to lock and pushing to the
                // queue. tuning the number of tries before pushing to the queue may marginally
                // improve performance.
                if try == 0 {
                    self.lock.queue.push(cx.waker().clone());
                } else {
                    return Ok(Async::Pending);
                }
            } else {
                break;
            }
        }
        Ok(Async::Ready(LockGuard {
            lock: self.lock,
        }))
    }
}
pub struct LockGuard<'a, T: 'a> {
    lock: &'a Lock<T>,
}
impl<'a, T: 'a> Drop for LockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.flag.store(false, Ordering::Release);

        // wake anyone that was waiting for the lock
        self.lock.queue.try_pop().map(|x| x.wake());
    }
}
impl<'a, T: 'a> Deref for LockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<'a, T: 'a> DerefMut for LockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
