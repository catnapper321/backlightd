use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};

#[derive(Debug, Clone, Copy)]
pub enum ClampedValue<T: Ord> {
    Min(T),
    Max(T),
    Intermediate(T),
}

impl<T: Ord> ClampedValue<T> {
    pub fn new(v: T, min: T, max: T) -> Self {
        if v >= max {
            return Self::Max(max);
        }
        if v <= min {
            return Self::Min(min);
        }
        Self::Intermediate(v)
    }
    pub fn is_max(&self) -> bool {
        matches!(self, Self::Max(_))
    }
    pub fn is_min(&self) -> bool {
        matches!(self, Self::Min(_))
    }
    pub fn is_intermediate(&self) -> bool {
        matches!(self, Self::Intermediate(_))
    }
    pub fn map<F, A>(self, f: F) -> ClampedValue<A>
    where
        F: FnOnce(T) -> A,
        A: Ord,
    {
        match self {
            ClampedValue::Min(x) => ClampedValue::Min(f(x)),
            ClampedValue::Max(x) => ClampedValue::Max(f(x)),
            ClampedValue::Intermediate(x) => ClampedValue::Intermediate(f(x)),
        }
    }
}

impl<T: Ord + Copy> ClampedValue<T> {
    pub fn value(&self) -> T {
        match *self {
            ClampedValue::Min(v) => v,
            ClampedValue::Max(v) => v,
            ClampedValue::Intermediate(v) => v,
        }
    }
    pub fn replace(&mut self, value: T) {
        *self = match self {
            ClampedValue::Min(_) => ClampedValue::Min(value),
            ClampedValue::Max(_) => ClampedValue::Max(value),
            ClampedValue::Intermediate(_) => ClampedValue::Intermediate(value),
        };
    }
    pub fn swap(&mut self, value: T) -> Self {
        let r = self.clone();
        self.replace(value);
        r
    }
}

impl<T: Ord> Deref for ClampedValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ClampedValue::Min(v) => v,
            ClampedValue::Max(v) => v,
            ClampedValue::Intermediate(v) => v,
        }
    }
}
// impl<T: Ord + Add<Output = T> + Copy> Add<T> for ClampedValue<T> {
//     type Output = ClampedValue<T>;

//     fn add(self, rhs: T) -> Self::Output {
//         self.map(|x| x + rhs)
//     }
// }
// impl<T: Ord + Add<Output = T> + Copy> AddAssign<T> for ClampedValue<T> {
//     fn add_assign(&mut self, rhs: T) {
//         *self = self.map(|x| x + rhs);
//     }
// }

// impl<T: Ord + Sub<Output = T> + Copy> Sub<T> for ClampedValue<T> {
//     type Output = ClampedValue<T>;

//     fn sub(self, rhs: T) -> Self::Output {
//         self.map(|x| x - rhs)
//     }
// }
// impl<T: Ord + Sub<Output = T> + Copy> SubAssign<T> for ClampedValue<T> {
//     fn sub_assign(&mut self, rhs: T) {
//         *self = self.map(|x| x - rhs);
//     }
// }
