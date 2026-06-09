//! 字节大小值对象。
//!
//! 内部 u64,提供人类可读格式化、K/M/G 互转。

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteSize(pub u64);

impl ByteSize {
    pub const ZERO: Self = Self(0);
    pub const KB: Self = Self(1024);
    pub const MB: Self = Self(1024 * 1024);
    pub const GB: Self = Self(1024 * 1024 * 1024);
    pub const TB: Self = Self(1024 * 1024 * 1024 * 1024);

    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> u64 {
        self.0
    }

    /// 人类可读格式(取 `humansize` 库)。
    pub fn humanize(&self) -> String {
        humansize::format_size(self.0, humansize::BINARY)
    }

    /// 校验:是否有足够空间(留 5% 缓冲)。
    pub fn fits_in(&self, available: ByteSize) -> bool {
        available.as_bytes() >= self.as_bytes() + self.as_bytes() / 20
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.humanize())
    }
}

impl std::ops::Add for ByteSize {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for ByteSize {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::iter::Sum for ByteSize {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

impl From<u64> for ByteSize {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl std::ops::Mul<u64> for ByteSize {
    type Output = Self;
    fn mul(self, rhs: u64) -> Self {
        Self(self.0 * rhs)
    }
}
