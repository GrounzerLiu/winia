use std::f32::consts::PI;
use std::ops::{Index, IndexMut, MulAssign};
use crate::offset::Offset;
use crate::point::Point;
use crate::rect::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub values: [f32; 16],
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            values: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]
        }
    }
}

impl Matrix {
    pub fn new(values: [f32; 16]) -> Self {
        Self { values }
    }

    /** Does the 3D transform on [point] and returns the `x` and `y` values in an [Offset]. */
    pub fn map_offset(&self, point: Offset) -> Offset {
        let v00 = self[(0, 0)];
        let v01 = self[(0, 1)];
        let v03 = self[(0, 3)];
        let v10 = self[(1, 0)];
        let v11 = self[(1, 1)];
        let v13 = self[(1, 3)];
        let v30 = self[(3, 0)];
        let v31 = self[(3, 1)];
        let v33 = self[(3, 3)];

        let x = point.x();
        let y = point.y();
        let z = v03 * x + v13 * y + v33;
        let inverse_z = 1.0 / z;
        let p_z = if inverse_z.is_finite() {
            inverse_z
        } else {
            0.0
        };

        Point(
            p_z * (v00 * x + v10 * y + v30),
            p_z * (v01 * x + v11 * y + v31)
        )
    }
    /** Does a 3D transform on [rect] and returns its bounds after the transform. */
    pub fn map_rect(&self, rect: Rect) -> Rect {
        let v00 = self[(0, 0)];
        let v01 = self[(0, 1)];
        let v03 = self[(0, 3)];
        let v10 = self[(1, 0)];
        let v11 = self[(1, 1)];
        let v13 = self[(1, 3)];
        let v30 = self[(3, 0)];
        let v31 = self[(3, 1)];
        let v33 = self[(3, 3)];

        let l = rect.left;
        let t = rect.top;
        let r = rect.right;
        let b = rect.bottom;

        let mut x = l;
        let mut y = t;
        let mut inverse_z = 1.0 / (v03 * x + v13 * y + v33);
        let mut p_z = if inverse_z.is_finite() {
            inverse_z
        } else {
            0.0
        };
        let x0 = p_z * (v00 * x + v10 * y + v30);
        let y0 = p_z * (v01 * x + v11 * y + v31);

        x = l;
        y = b;
        inverse_z = 1.0 / (v03 * x + v13 * y + v33);
        p_z = if inverse_z.is_finite() {
            inverse_z
        } else {
            0.0
        };
        let x1 = p_z * (v00 * x + v10 * y + v30);
        let y1 = p_z * (v01 * x + v11 * y + v31);

        x = r;
        y = t;
        inverse_z = 1.0 / (v03 * x + v13 * y + v33);
        p_z = if inverse_z.is_finite() {
            inverse_z
        } else {
            0.0
        };
        let x2 = p_z * (v00 * x + v10 * y + v30);
        let y2 = p_z * (v01 * x + v11 * y + v31);

        x = r;
        y = b;
        inverse_z = 1.0 / (v03 * x + v13 * y + v33);
        p_z = if inverse_z.is_finite() {
            inverse_z
        } else {
            0.0
        };
        let x3 = p_z * (v00 * x + v10 * y + v30);
        let y3 = p_z * (v01 * x + v11 * y + v31);

        Rect {
            left: x0.min(x1).min(x2).min(x3),
            top: y0.min(y1).min(y2).min(y3),
            right: x0.max(x1).max(x2).max(x3),
            bottom: y0.max(y1).max(y2).max(y3),
        }
    }

    pub fn inverse(&mut self) {
        let a00 = self[(0, 0)];
        let a01 = self[(0, 1)];
        let a02 = self[(0, 2)];
        let a03 = self[(0, 3)];
        let a10 = self[(1, 0)];
        let a11 = self[(1, 1)];
        let a12 = self[(1, 2)];
        let a13 = self[(1, 3)];
        let a20 = self[(2, 0)];
        let a21 = self[(2, 1)];
        let a22 = self[(2, 2)];
        let a23 = self[(2, 3)];
        let a30 = self[(3, 0)];
        let a31 = self[(3, 1)];
        let a32 = self[(3, 2)];
        let a33 = self[(3, 3)];

        let b00 = a00 * a11 - a01 * a10;
        let b01 = a00 * a12 - a02 * a10;
        let b02 = a00 * a13 - a03 * a10;
        let b03 = a01 * a12 - a02 * a11;
        let b04 = a01 * a13 - a03 * a11;
        let b05 = a02 * a13 - a03 * a12;
        let b06 = a20 * a31 - a21 * a30;
        let b07 = a20 * a32 - a22 * a30;
        let b08 = a20 * a33 - a23 * a30;
        let b09 = a21 * a32 - a22 * a31;
        let b10 = a21 * a33 - a23 * a31;
        let b11 = a22 * a33 - a23 * a32;

        let det = b00 * b11 - b01 * b10 + b02 * b09 + b03 * b08 - b04 * b07 + b05 * b06;
        if det == 0.0 {
            return;
        }

        let inv_det = 1.0 / det;
        self[(0, 0)] = (a11 * b11 - a12 * b10 + a13 * b09) * inv_det;
        self[(0, 1)] = (-a01 * b11 + a02 * b10 - a03 * b09) * inv_det;
        self[(0, 2)] = (a31 * b05 - a32 * b04 + a33 * b03) * inv_det;
        self[(0, 3)] = (-a21 * b05 + a22 * b04 - a23 * b03) * inv_det;
        self[(1, 0)] = (-a10 * b11 + a12 * b08 - a13 * b07) * inv_det;
        self[(1, 1)] = (a00 * b11 - a02 * b08 + a03 * b07) * inv_det;
        self[(1, 2)] = (-a30 * b05 + a32 * b02 - a33 * b01) * inv_det;
        self[(1, 3)] = (a20 * b05 - a22 * b02 + a23 * b01) * inv_det;
        self[(2, 0)] = (a10 * b10 - a11 * b08 + a13 * b06) * inv_det;
        self[(2, 1)] = (-a00 * b10 + a01 * b08 - a03 * b06) * inv_det;
        self[(2, 2)] = (a30 * b04 - a31 * b02 + a33 * b00) * inv_det;
        self[(2, 3)] = (-a20 * b04 + a21 * b02 - a23 * b00) * inv_det;
        self[(3, 0)] = (-a10 * b09 + a11 * b07 - a12 * b06) * inv_det;
        self[(3, 1)] = (a00 * b09 - a01 * b07 + a02 * b06) * inv_det;
        self[(3, 2)] = (-a30 * b03 + a31 * b01 - a32 * b00) * inv_det;
        self[(3, 3)] = (a20 * b03 - a21 * b01 + a22 * b00) * inv_det;
    }

    pub fn reset(&mut self) {
        self.values = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0
        ]
    }

    pub fn set_from(&mut self, matrix: impl AsRef<Matrix>) {
        let m = matrix.as_ref();
        for i in 0..16 {
            self.values[i] = m.values[i];
        }
    }

    pub fn rotate_x(&mut self, degrees: f32) {
        let r = degrees * (PI / 180.0);
        let s = r.sin();
        let c = r.cos();

        let a01 = self[(0, 1)];
        let a02 = self[(0, 2)];
        let v01 = a01 * c + a02 * s;
        let v02 = -a01 * s + a02 * c;

        let a11 = self[(1, 1)];
        let a12 = self[(1, 2)];
        let v11 = a11 * c + a12 * s;
        let v12 = -a11 * s + a12 * c;

        let a21 = self[(2, 1)];
        let a22 = self[(2, 2)];
        let v21 = a21 * c + a22 * s;
        let v22 = -a21 * s + a22 * c;

        let a31 = self[(3, 1)];
        let a32 = self[(3, 2)];
        let v31 = a31 * c + a32 * s;
        let v32 = -a31 * s + a32 * c;

        self[(0, 1)] = v01;
        self[(0, 2)] = v02;
        self[(1, 1)] = v11;
        self[(1, 2)] = v12;
        self[(2, 1)] = v21;
        self[(2, 2)] = v22;
        self[(3, 1)] = v31;
        self[(3, 2)] = v32;
    }

    pub fn rotate_y(&mut self, degrees: f32) {
        let r = degrees * (PI / 180.0);
        let s = r.sin();
        let c = r.cos();

        let a00 = self[(0, 0)];
        let a02 = self[(0, 2)];
        let v00 = a00 * c - a02 * s;
        let v02 = a00 * s + a02 * c;

        let a10 = self[(1, 0)];
        let a12 = self[(1, 2)];
        let v10 = a10 * c - a12 * s;
        let v12 = a10 * s + a12 * c;

        let a20 = self[(2, 0)];
        let a22 = self[(2, 2)];
        let v20 = a20 * c - a22 * s;
        let v22 = a20 * s + a22 * c;

        let a30 = self[(3, 0)];
        let a32 = self[(3, 2)];
        let v30 = a30 * c - a32 * s;
        let v32 = a30 * s + a32 * c;

        self[(0, 0)] = v00;
        self[(0, 2)] = v02;
        self[(1, 0)] = v10;
        self[(1, 2)] = v12;
        self[(2, 0)] = v20;
        self[(2, 2)] = v22;
        self[(3, 0)] = v30;
        self[(3, 2)] = v32;
    }

    pub fn rotate_z(&mut self, degrees: f32) {
        let r = degrees * (PI / 180.0);
        let s = r.sin();
        let c = r.cos();

        let a00 = self[(0, 0)];
        let a10 = self[(1, 0)];
        let v00 = c * a00 + s * a10;
        let v10 = -s * a00 + c * a10;

        let a01 = self[(0, 1)];
        let a11 = self[(1, 1)];
        let v01 = c * a01 + s * a11;
        let v11 = -s * a01 + c * a11;

        let a02 = self[(0, 2)];
        let a12 = self[(1, 2)];
        let v02 = c * a02 + s * a12;
        let v12 = -s * a02 + c * a12;

        let a03 = self[(0, 3)];
        let a13 = self[(1, 3)];
        let v03 = c * a03 + s * a13;
        let v13 = -s * a03 + c * a13;

        self[(0, 0)] = v00;
        self[(0, 1)] = v01;
        self[(0, 2)] = v02;
        self[(0, 3)] = v03;
        self[(1, 0)] = v10;
        self[(1, 1)] = v11;
        self[(1, 2)] = v12;
        self[(1, 3)] = v13;
    }

    pub fn scale(&mut self, x: f32, y: f32, z: f32) {
        self[(0, 0)] *= x;
        self[(0, 1)] *= x;
        self[(0, 2)] *= x;
        self[(0, 3)] *= x;
        self[(1, 0)] *= y;
        self[(1, 1)] *= y;
        self[(1, 2)] *= y;
        self[(1, 3)] *= y;
        self[(2, 0)] *= z;
        self[(2, 1)] *= z;
        self[(2, 2)] *= z;
        self[(2, 3)] *= z;
    }

    pub fn translate(
        &mut self,
        x: impl Into<Option<f32>>,
        y: impl Into<Option<f32>>,
        z: impl Into<Option<f32>>
    ) {
        let x = x.into().unwrap_or(0.0);
        let y = y.into().unwrap_or(0.0);
        let z = z.into().unwrap_or(0.0);
        let t1 = self[(0, 0)] * x + self[(1, 0)] * y + self[(2, 0)] * z + self[(3, 0)];
        let t2 = self[(0, 1)] * x + self[(1, 1)] * y + self[(2, 1)] * z + self[(3, 1)];
        let t3 = self[(0, 2)] * x + self[(1, 2)] * y + self[(2, 2)] * z + self[(3, 2)];
        let t4 = self[(0, 3)] * x + self[(1, 3)] * y + self[(2, 3)] * z + self[(3, 3)];
        self[(3, 0)] = t1;
        self[(3, 1)] = t2;
        self[(3, 2)] = t3;
        self[(3, 3)] = t4;
    }
}

impl MulAssign<&Matrix> for Matrix {
    fn mul_assign(&mut self, rhs: &Matrix) {
        if self.values.len() != 16 || rhs.values.len() != 16 {
            return;
        }
        let v00 = dot(self, 0, rhs, 0);
        let v01 = dot(self, 0, rhs, 1);
        let v02 = dot(self, 0, rhs, 2);
        let v03 = dot(self, 0, rhs, 3);
        let v10 = dot(self, 1, rhs, 0);
        let v11 = dot(self, 1, rhs, 1);
        let v12 = dot(self, 1, rhs, 2);
        let v13 = dot(self, 1, rhs, 3);
        let v20 = dot(self, 2, rhs, 0);
        let v21 = dot(self, 2, rhs, 1);
        let v22 = dot(self, 2, rhs, 2);
        let v23 = dot(self, 2, rhs, 3);
        let v30 = dot(self, 3, rhs, 0);
        let v31 = dot(self, 3, rhs, 1);
        let v32 = dot(self, 3, rhs, 2);
        let v33 = dot(self, 3, rhs, 3);

        let v = &mut self.values;
        v[0] = v00;
        v[1] = v01;
        v[2] = v02;
        v[3] = v03;
        v[4] = v10;
        v[5] = v11;
        v[6] = v12;
        v[7] = v13;
        v[8] = v20;
        v[9] = v21;
        v[10] = v22;
        v[11] = v23;
        v[12] = v30;
        v[13] = v31;
        v[14] = v32;
        v[15] = v33;
    }
}

#[inline]
fn dot(m1: &Matrix, row: usize, m2: &Matrix, column: usize) -> f32 {
    m1[(row, 0)] * m2[(0, column)] +
    m1[(row, 1)] * m2[(1, column)] +
    m1[(row, 2)] * m2[(2, column)] +
    m1[(row, 3)] * m2[(3, column)]
}

impl Index<(usize, usize)> for Matrix {
    type Output = f32;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        let (row, col) = index;
        &self.values[row * 4 + col]
    }
}

impl IndexMut<(usize, usize)> for Matrix {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        let (row, col) = index;
        &mut self.values[row * 4 + col]
    }
}

impl AsRef<Matrix> for Matrix {
    fn as_ref(&self) -> &Matrix {
        self
    }
}