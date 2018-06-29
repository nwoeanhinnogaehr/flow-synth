use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Pt2 {
    pub x: f32,
    pub y: f32,
}
impl Pt2 {
    pub fn new(x: f32, y: f32) -> Pt2 {
        Pt2 {
            x,
            y,
        }
    }
    pub fn zero() -> Pt2 {
        0.0.into()
    }
    pub fn with_z(self, z: f32) -> Pt3 {
        Pt3::new(self.x, self.y, z)
    }
}
impl From<Pt2> for [f32; 2] {
    fn from(val: Pt2) -> [f32; 2] {
        [val.x, val.y]
    }
}
impl From<f32> for Pt2 {
    fn from(val: f32) -> Pt2 {
        Pt2::new(val, val)
    }
}
macro_rules! impl_binop_pt2 {
    ($op:ident, $fn:ident) => {
        impl $op for Pt2 {
            type Output = Pt2;
            #[inline(always)]
            fn $fn(self, other: Pt2) -> Pt2 {
                Pt2::new(self.x.$fn(other.x), self.y.$fn(other.y))
            }
        }
        impl $op<f32> for Pt2 {
            type Output = Pt2;
            #[inline(always)]
            fn $fn(self, other: f32) -> Pt2 {
                Pt2::new(self.x.$fn(other), self.y.$fn(other))
            }
        }
    };
}
macro_rules! impl_uniop_pt2 {
    ($op:ty, $fn:ident) => {
        impl $op for Pt2 {
            type Output = Pt2;
            #[inline(always)]
            fn $fn(self) -> Pt2 {
                Pt2::new((self.x).$fn(), (self.y).$fn())
            }
        }
    };
}
impl_binop_pt2!(Add, add);
impl_binop_pt2!(Sub, sub);
impl_binop_pt2!(Mul, mul);
impl_binop_pt2!(Div, div);
impl_uniop_pt2!(Neg, neg);

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Pt3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Pt3 {
    pub fn new(x: f32, y: f32, z: f32) -> Pt3 {
        Pt3 {
            x,
            y,
            z,
        }
    }
    pub fn zero() -> Pt3 {
        0.0.into()
    }
    pub fn drop_z(self) -> Pt2 {
        Pt2::new(self.x, self.y)
    }
    pub fn with_z(self, z: f32) -> Pt3 {
        Pt3::new(self.x, self.y, z)
    }
}
impl From<Pt3> for [f32; 3] {
    fn from(val: Pt3) -> [f32; 3] {
        [val.x, val.y, val.z]
    }
}
impl From<f32> for Pt3 {
    fn from(val: f32) -> Pt3 {
        Pt3::new(val, val, val)
    }
}
macro_rules! impl_binop_pt3 {
    ($op:ident, $fn:ident) => {
        impl $op for Pt3 {
            type Output = Pt3;
            #[inline(always)]
            fn $fn(self, other: Pt3) -> Pt3 {
                Pt3::new(
                    self.x.$fn(other.x),
                    self.y.$fn(other.y),
                    self.z.$fn(other.z),
                )
            }
        }
        impl $op<f32> for Pt3 {
            type Output = Pt3;
            #[inline(always)]
            fn $fn(self, other: f32) -> Pt3 {
                Pt3::new(self.x.$fn(other), self.y.$fn(other), self.z.$fn(other))
            }
        }
    };
}
macro_rules! impl_uniop_pt3 {
    ($op:ty, $fn:ident) => {
        impl $op for Pt3 {
            type Output = Pt3;
            #[inline(always)]
            fn $fn(self) -> Pt3 {
                Pt3::new((self.x).$fn(), (self.y).$fn(), (self.z).$fn())
            }
        }
    };
}
impl_binop_pt3!(Add, add);
impl_binop_pt3!(Sub, sub);
impl_binop_pt3!(Mul, mul);
impl_binop_pt3!(Div, div);
impl_uniop_pt3!(Neg, neg);

#[derive(Copy, Clone, Debug, Default)]
pub struct Rect2 {
    pub pos: Pt2,
    pub size: Pt2,
}

impl Rect2 {
    pub fn new(pos: Pt2, size: Pt2) -> Rect2 {
        Rect2 {
            pos,
            size,
        }
    }
    pub fn with_z(self, z: f32) -> Rect3 {
        Rect3 {
            pos: Pt3::new(self.pos.x, self.pos.y, z),
            size: self.size,
        }
    }
    /// upgrade to rect3 with z from other
    pub fn with_z_from(self, other: &Rect3) -> Rect3 {
        Rect3 {
            pos: Pt3::new(self.pos.x, self.pos.y, other.pos.z),
            size: self.size,
        }
    }
    pub fn intersect(&self, pt: Pt2) -> bool {
        pt.x > self.pos.x
            && pt.x < self.pos.x + self.size.x
            && pt.y > self.pos.y
            && pt.y < self.pos.y + self.size.y
    }
    pub fn offset(self, other: Rect2) -> Rect2 {
        Rect2::new(self.pos + other.pos, self.size)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect3 {
    pub pos: Pt3,
    pub size: Pt2,
}
impl Rect3 {
    pub fn new(pos: Pt3, size: Pt2) -> Rect3 {
        Rect3 {
            pos,
            size,
        }
    }
    pub fn drop_z(self) -> Rect2 {
        Rect2::new(self.pos.drop_z(), self.size)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Box3 {
    pub pos: Pt3,
    pub size: Pt3,
}
impl Box3 {
    pub fn new(pos: Pt3, size: Pt3) -> Box3 {
        Box3 {
            pos,
            size,
        }
    }
    pub fn flatten(self) -> Rect3 {
        Rect3::new(self.pos, self.size.drop_z())
    }
}
