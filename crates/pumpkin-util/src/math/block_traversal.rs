//! Vanilla `BlockGetter.forEachBlockIntersectedBetween`: the blocks a box passes through when it
//! moves along a vector, in vanilla's order and with vanilla's quirks (`getFurthestCorner`).

use std::collections::HashSet;

use super::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};

/// Vanilla's `1.0E-5F` widened to a double.
const EPSILON: f64 = 9.999_999_747_378_752E-6;

/// Vanilla `AABB.clip(minX, ..., from, to)`: where the segment enters the box, if it does.
/// A segment that starts inside the box does not enter it.
#[must_use]
pub fn clip(
    bounding_box: &BoundingBox,
    from: Vector3<f64>,
    to: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let min = [bounding_box.min.x, bounding_box.min.y, bounding_box.min.z];
    let max = [bounding_box.max.x, bounding_box.max.y, bounding_box.max.z];
    let origin = [from.x, from.y, from.z];
    let delta = [to.x - from.x, to.y - from.y, to.z - from.z];
    let mut scale = 1.0;
    let mut hit = false;
    for axis in 0..3 {
        if delta[axis] > 1.0e-7 {
            hit |= clip_point(&mut scale, axis, min[axis], &origin, &delta, &min, &max);
        } else if delta[axis] < -1.0e-7 {
            hit |= clip_point(&mut scale, axis, max[axis], &origin, &delta, &min, &max);
        }
    }
    hit.then(|| from + (to - from) * scale)
}

fn clip_point(
    scale: &mut f64,
    axis: usize,
    plane: f64,
    origin: &[f64; 3],
    delta: &[f64; 3],
    min: &[f64; 3],
    max: &[f64; 3],
) -> bool {
    let s = (plane - origin[axis]) / delta[axis];
    if s <= 0.0 || s >= *scale {
        return false;
    }
    let b = (axis + 1) % 3;
    let c = (axis + 2) % 3;
    let pb = origin[b] + s * delta[b];
    let pc = origin[c] + s * delta[c];
    if min[b] - 1.0e-7 < pb && pb < max[b] + 1.0e-7 && min[c] - 1.0e-7 < pc && pc < max[c] + 1.0e-7
    {
        *scale = s;
        return true;
    }
    false
}

/// Vanilla `AABB.collidedAlongVector`: whether `moving`, swept by `delta`, touches any of `shapes`.
#[must_use]
pub fn collided_along_vector(
    moving: &BoundingBox,
    delta: Vector3<f64>,
    shapes: impl Iterator<Item = BoundingBox>,
) -> bool {
    let center = (moving.min + moving.max) * 0.5;
    let end = center + delta;
    let half = (moving.max - moving.min) * 0.5 - Vector3::new(1.0e-7, 1.0e-7, 1.0e-7);
    let contains = |b: &BoundingBox, p: Vector3<f64>| {
        p.x >= b.min.x
            && p.x < b.max.x
            && p.y >= b.min.y
            && p.y < b.max.y
            && p.z >= b.min.z
            && p.z < b.max.z
    };
    shapes.into_iter().any(|shape| {
        let inflated = BoundingBox::new(shape.min - half, shape.max + half);
        contains(&inflated, end)
            || contains(&inflated, center)
            || inflated.ray_intersection(center, delta).is_some()
    })
}

/// Vanilla `BlockPos.betweenCornersInDirection(AABB, Vec3)`: the blocks of the box.
fn box_blocks(bounding_box: &BoundingBox) -> impl Iterator<Item = BlockPos> {
    BlockPos::iterate(
        BlockPos::floored_v(bounding_box.min),
        BlockPos::floored_v(bounding_box.max),
    )
}

/// Vanilla `Mth.sign`.
fn sign(value: f64) -> i32 {
    if value == 0.0 {
        0
    } else if value > 0.0 {
        1
    } else {
        -1
    }
}

/// Vanilla `Mth.frac`.
fn frac(value: f64) -> f64 {
    value - value.floor()
}

/// Vanilla `BlockGetter.getFurthestCorner`, axes mixed up as in vanilla.
fn furthest_corner(travel: Vector3<f64>) -> (i32, i32, i32) {
    let (ax, ay, az) = (travel.x.abs(), travel.y.abs(), travel.z.abs());
    let sx = if travel.x >= 0.0 { 1 } else { -1 };
    let sy = if travel.y >= 0.0 { 1 } else { -1 };
    let sz = if travel.z >= 0.0 { 1 } else { -1 };
    if ax <= ay && ax <= az {
        (-sx, -sz, sy)
    } else if ay <= az {
        (sz, -sy, -sx)
    } else {
        (-sy, sx, -sz)
    }
}

/// Vanilla `addCollisionsAlongTravel`: walks the box's leading corner through the block grid
/// and visits the blocks of the box's face at each step. The step count, or `None` once
/// `visit` returns `false`.
fn add_collisions_along_travel(
    seen: &mut HashSet<BlockPos>,
    travel: Vector3<f64>,
    aabb: &BoundingBox,
    visit: &mut impl FnMut(BlockPos, i32) -> bool,
) -> Option<i32> {
    let size = aabb.max - aabb.min;
    let corner = furthest_corner(travel);
    let center = (aabb.min + aabb.max) * 0.5;
    let end = Vector3::new(
        center.x + size.x * 0.5 * f64::from(corner.0),
        center.y + size.y * 0.5 * f64::from(corner.1),
        center.z + size.z * 0.5 * f64::from(corner.2),
    );
    let start = end - travel;
    let (mut x, mut y, mut z) = (
        start.x.floor() as i32,
        start.y.floor() as i32,
        start.z.floor() as i32,
    );
    let (sx, sy, sz) = (sign(travel.x), sign(travel.y), sign(travel.z));
    let step = |s: i32, t: f64| {
        if s == 0 { f64::MAX } else { f64::from(s) / t }
    };
    let (dx, dy, dz) = (step(sx, travel.x), step(sy, travel.y), step(sz, travel.z));
    let first = |s: i32, d: f64, p: f64| d * if s > 0 { 1.0 - frac(p) } else { frac(p) };
    let mut tx = first(sx, dx, start.x);
    let mut ty = first(sy, dy, start.y);
    let mut tz = first(sz, dz, start.z);
    let mut steps = 0;
    while tx <= 1.0 || ty <= 1.0 || tz <= 1.0 {
        if tx < ty {
            if tx < tz {
                x += sx;
                tx += dx;
            } else {
                z += sz;
                tz += dz;
            }
        } else if ty < tz {
            y += sy;
            ty += dy;
        } else {
            z += sz;
            tz += dz;
        }
        let voxel = BoundingBox::new(
            Vector3::new(f64::from(x), f64::from(y), f64::from(z)),
            Vector3::new(f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)),
        );
        let Some(hit) = clip(&voxel, start, end) else {
            continue;
        };
        steps += 1;
        let px = hit
            .x
            .clamp(f64::from(x) + EPSILON, f64::from(x) + 1.0 - EPSILON);
        let py = hit
            .y
            .clamp(f64::from(y) + EPSILON, f64::from(y) + 1.0 - EPSILON);
        let pz = hit
            .z
            .clamp(f64::from(z) + EPSILON, f64::from(z) + 1.0 - EPSILON);
        let other = BlockPos::new(
            (px - size.x * f64::from(corner.0)).floor() as i32,
            (py - size.y * f64::from(corner.1)).floor() as i32,
            (pz - size.z * f64::from(corner.2)).floor() as i32,
        );
        for pos in BlockPos::iterate(BlockPos::new(x, y, z), other) {
            if seen.insert(pos) && !visit(pos, steps) {
                return None;
            }
        }
    }
    Some(steps)
}

/// Vanilla `BlockGetter.forEachBlockIntersectedBetween(from, to, aabb, visitor)`, where `aabb`
/// is the box at `to`. `false` as soon as `visit` returns `false`.
pub fn for_each_block_intersected_between(
    from: Vector3<f64>,
    to: Vector3<f64>,
    aabb: &BoundingBox,
    mut visit: impl FnMut(BlockPos, i32) -> bool,
) -> bool {
    let travel = to - from;
    if travel.length_squared() < f64::from(1.0e-5f32 * 1.0e-5f32) {
        return box_blocks(aabb).all(|pos| visit(pos, 0));
    }
    let mut seen = HashSet::new();
    for pos in box_blocks(&aabb.shift(travel * -1.0)) {
        if !visit(pos, 0) {
            return false;
        }
        seen.insert(pos);
    }
    let Some(steps) = add_collisions_along_travel(&mut seen, travel, aabb, &mut visit) else {
        return false;
    };
    for pos in box_blocks(aabb) {
        if seen.insert(pos) && !visit(pos, steps + 1) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visited(from: Vector3<f64>, to: Vector3<f64>, size: f64) -> HashSet<BlockPos> {
        let aabb = BoundingBox::new(
            Vector3::new(to.x - size / 2.0, to.y, to.z - size / 2.0),
            Vector3::new(to.x + size / 2.0, to.y + size, to.z + size / 2.0),
        );
        let mut out = HashSet::new();
        for_each_block_intersected_between(from, to, &aabb, |pos, _| {
            out.insert(pos);
            true
        });
        out
    }

    #[test]
    fn a_still_box_visits_its_own_blocks() {
        let at = Vector3::new(0.5, 0.0, 0.5);
        let blocks = visited(at, at, 0.5);
        assert_eq!(blocks, HashSet::from([BlockPos::new(0, 0, 0)]));
    }

    #[test]
    fn a_moving_box_visits_the_blocks_on_its_way() {
        let blocks = visited(
            Vector3::new(0.5, 0.0, 0.5),
            Vector3::new(4.5, 0.0, 0.5),
            0.5,
        );
        for x in 0..=4 {
            assert!(blocks.contains(&BlockPos::new(x, 0, 0)), "missing x {x}");
        }
        assert!(!blocks.contains(&BlockPos::new(2, 1, 0)));
    }

    #[test]
    fn a_false_visit_stops_the_walk() {
        let aabb = BoundingBox::new(Vector3::new(4.0, 0.0, 0.0), Vector3::new(5.0, 1.0, 1.0));
        let mut last = None;
        let all = for_each_block_intersected_between(
            Vector3::new(0.5, 0.0, 0.5),
            Vector3::new(4.5, 0.0, 0.5),
            &aabb,
            |pos, _| {
                last = Some(pos);
                pos.0.x < 2
            },
        );
        assert!(!all);
        assert_eq!(
            last.map(|pos| pos.0.x),
            Some(2),
            "stops at the first refusal"
        );
    }
}
