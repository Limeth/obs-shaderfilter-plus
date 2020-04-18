use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};

pub struct Indexed<T> {
    pub index: usize,
    pub inner: T,
}

impl<T> Indexed<T> {
    pub fn into_tuple(self) -> (usize, T) {
        (self.index, self.inner)
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn map<R>(self, map: impl FnOnce(T) -> R) -> Indexed<R> {
        Indexed {
            index: self.index,
            inner: (map)(self.inner),
        }
    }
}

impl<T> Indexed<Option<T>> {
    pub fn transpose(self) -> Option<Indexed<T>> {
        let Indexed { index, inner } = self;
        inner.map(|inner| {
            Indexed { index, inner }
        })
    }
}

impl<T, E> Indexed<Result<T, E>> {
    pub fn transpose(self) -> Result<Indexed<T>, E> {
        let Indexed { index, inner } = self;
        inner.map(|inner| {
            Indexed { index, inner }
        })
    }
}

impl<T> From<(usize, T)> for Indexed<T> {
    fn from((index, inner): (usize, T)) -> Self {
        Self { index, inner }
    }
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Indexed<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> PartialEq for Indexed<T> {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.index, &other.index)
    }
}

impl<T> Eq for Indexed<T> {
}

impl<T> PartialOrd for Indexed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&self.index, &other.index)
    }
}

impl<T> Ord for Indexed<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.index, &other.index)
    }
}
