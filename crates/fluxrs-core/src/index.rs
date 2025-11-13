use std::ops::{Add, Deref, Index as StdIndex};

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct Index {
    pub count: usize,
}
impl Index {
    pub fn increment(&mut self) {
        self.count += 1;
    }
    pub fn decrement(&mut self) {
        self.count -= 1;
    }
    pub fn reset(&mut self) {
        self.count = 0;
    }
    pub fn set(&mut self, val: usize) {
        self.count = val;
    }
}
impl Default for Index {
    fn default() -> Self {
        Self { count: 0 }
    }
}

impl Deref for Index {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.count
    }
}
impl Add for Index {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
        }
    }
}
impl<T> StdIndex<Index> for Vec<T> {
    type Output = T;

    fn index(&self, index: Index) -> &Self::Output {
        &self[index.count]
    }
}
