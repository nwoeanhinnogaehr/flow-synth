use futures::prelude::*;

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
